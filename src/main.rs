use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use jiff::Zoned;
use ratatui::{
    crossterm::{
        cursor::SetCursorStyle,
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event as TermEvent, KeyEventKind,
            MouseButton, MouseEventKind,
        },
        execute,
    },
    layout::Rect,
};
use tokio::sync::mpsc;
use tuilook::accounts::{Account, Accounts};
use tuilook::auth::{
    KeyringStore, MemoryStore, OAuthClient, OAuthConfig, SecretStore, TokenProvider,
};
use tuilook::core::{run_engine, sync_folders, sync_messages};
use tuilook::graph::{GRAPH_V1, GraphBackend, GraphClient};
use tuilook::model::MailBackend;
use tuilook::store::Store;
use tuilook::tui::{
    app::{AppState, Mouse},
    compose::Field,
    demo,
    dialog::Dialog,
    keymap::Keymap,
    mailboxes::{Mailbox, Mailboxes},
    session, ui,
    vim::VimMode,
};

/// Delegated Graph scopes; calendar and contacts can be added post-v1.
const SCOPES: [&str; 5] = [
    "offline_access",
    "User.Read",
    "Mail.ReadWrite",
    "Mail.Send",
    "MailboxSettings.Read",
];

/// Keychain account the refresh token was stored under before tuilook knew
/// several accounts; now each is stored under its address.
const LEGACY_ACCOUNT: &str = "default";

/// tuilook's own Entra ID app registration (a public client; not a secret).
const DEFAULT_CLIENT_ID: &str = "5de297ab-80a5-4cfc-bdea-38e33b917993";

#[derive(Parser)]
#[command(
    name = "otlk",
    version,
    about = "A terminal mail client for Microsoft 365"
)]
struct Cli {
    /// Entra ID application (client) ID to sign in with; use your own if your
    /// tenant blocks tuilook's. Each account remembers the one it signed in with.
    #[arg(long, env = "TUILOOK_CLIENT_ID", default_value = DEFAULT_CLIENT_ID, global = true)]
    client_id: Option<String>,
    /// Tenant to sign in to: consumers (personal accounts), organizations, common
    /// or a tenant ID. The default app registration only accepts consumers.
    /// Each account remembers the one it signed in to.
    #[arg(
        long,
        env = "TUILOOK_TENANT",
        default_value = "consumers",
        global = true
    )]
    tenant: String,
    /// Browse a built-in sample mailbox instead of signing in.
    #[arg(long)]
    demo: bool,
    /// Seconds between checks for new mail; 0 checks only at start and on F5.
    #[arg(long, env = "TUILOOK_SYNC_EVERY", default_value_t = 60, global = true)]
    sync_every: u64,
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Sign in with a device code. Run it again to add another account.
    Login,
    /// Forget an account's sign-in and cached mail.
    Logout {
        /// The account's address, or the start of it. Needed when signed in
        /// to several accounts.
        account: Option<String>,
    },
    /// List the signed-in accounts.
    Accounts,
    /// Sync every account's folders once, without the UI.
    Sync,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Runtime::new()?;
    match &cli.command {
        Some(CliCommand::Login) => runtime.block_on(login(&cli)),
        Some(CliCommand::Logout { account }) => runtime.block_on(logout(&cli, account.as_deref())),
        Some(CliCommand::Accounts) => {
            let accounts = runtime.block_on(signed_in(&cli))?;
            if accounts.0.is_empty() {
                println!("Not signed in.");
            }
            for account in accounts.0 {
                println!("{}", account.address);
            }
            Ok(())
        }
        Some(CliCommand::Sync) => runtime.block_on(sync_once(&cli)),
        None if cli.demo => {
            let mut mailboxes = Vec::new();
            for (index, (address, backend)) in demo::accounts().into_iter().enumerate() {
                let path = std::env::temp_dir().join(format!("tuilook-demo-{index}.db"));
                remove_db(&path)?;
                mailboxes.push((address, backend, path));
            }
            run_ui(&runtime, mailboxes, None, None, sync_every(&cli))
        }
        None => {
            let accounts = runtime.block_on(signed_in(&cli))?;
            if accounts.0.is_empty() {
                bail!("not signed in; run `otlk login` first");
            }
            let mailboxes = accounts
                .0
                .iter()
                .map(|account| {
                    Ok((
                        account.address.clone(),
                        graph_backend(account)?,
                        cache_path(&account.address)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            run_ui(
                &runtime,
                mailboxes,
                Some(&accounts_path()?),
                Some(&session_path()?),
                sync_every(&cli),
            )
        }
    }
}

fn client_id(cli: &Cli) -> Result<String> {
    match cli.client_id.clone().filter(|id| !id.is_empty()) {
        Some(client_id) => Ok(client_id),
        None => bail!(
            "no client ID: register a public client app in Entra ID, then pass \
             --client-id or set TUILOOK_CLIENT_ID"
        ),
    }
}

fn oauth(tenant: &str, client_id: &str) -> OAuthClient {
    OAuthClient::new(OAuthConfig {
        authority: "https://login.microsoftonline.com".to_owned(),
        tenant: tenant.to_owned(),
        client_id: client_id.to_owned(),
        scopes: SCOPES.map(str::to_owned).to_vec(),
    })
}

fn graph_backend(account: &Account) -> Result<GraphBackend<Arc<TokenProvider>>> {
    let secrets = Arc::new(KeyringStore::new(&account.address)?);
    let tokens = TokenProvider::new(oauth(&account.tenant, &account.client_id), secrets);
    Ok(backend_over(tokens))
}

fn backend_over(tokens: TokenProvider) -> GraphBackend<Arc<TokenProvider>> {
    GraphBackend::new(GraphClient::new(GRAPH_V1, Arc::new(tokens)))
}

/// Signs in, keeping the refresh token in memory until Graph says whose it is.
async fn login(cli: &Cli) -> Result<()> {
    let mut accounts = signed_in(cli).await?;
    let client_id = client_id(cli)?;
    let memory = Arc::new(MemoryStore::default());
    let tokens = TokenProvider::new(oauth(&cli.tenant, &client_id), memory.clone());
    tokens.login(|code| println!("{}", code.message)).await?;
    let address = backend_over(tokens)
        .address()
        .await?
        .context("Microsoft did not say which account signed in")?;
    let refresh_token = memory
        .load()?
        .context("Microsoft did not let tuilook stay signed in (no refresh token)")?;
    KeyringStore::new(&address)?.save(&refresh_token)?;
    accounts.upsert(Account {
        address: address.clone(),
        tenant: cli.tenant.clone(),
        client_id,
        signature: String::new(),
    });
    accounts.save(&accounts_path()?)?;
    println!("Signed in as {address}.");
    Ok(())
}

async fn logout(cli: &Cli, query: Option<&str>) -> Result<()> {
    let mut accounts = signed_in(cli).await?;
    let account = match (query, &accounts.0[..]) {
        (Some(query), _) => accounts.find(query)?.clone(),
        (None, [only]) => only.clone(),
        (None, []) => {
            println!("Not signed in.");
            return Ok(());
        }
        (None, _) => bail!("signed in to several accounts; say which: `otlk logout <address>`"),
    };
    KeyringStore::new(&account.address)?.clear()?;
    accounts.remove(&account.address);
    accounts.save(&accounts_path()?)?;
    match std::fs::remove_dir_all(account_dir(&account.address)?) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    println!("Signed out of {}.", account.address);
    Ok(())
}

/// The signed-in accounts, after moving a sign-in from an older tuilook.
async fn signed_in(cli: &Cli) -> Result<Accounts> {
    let path = accounts_path()?;
    if path.exists() {
        return Ok(Accounts::load(&path)?);
    }
    let accounts = migrate_legacy(cli).await?;
    if !accounts.0.is_empty() {
        accounts.save(&path)?;
        KeyringStore::new(LEGACY_ACCOUNT)?.clear()?;
    }
    Ok(accounts)
}

/// Files the one sign-in and cache of an older tuilook under the address
/// they belong to.
async fn migrate_legacy(cli: &Cli) -> Result<Accounts> {
    let Some(refresh_token) = KeyringStore::new(LEGACY_ACCOUNT)?.load()? else {
        return Ok(Accounts::default());
    };
    let client_id = client_id(cli)?;
    let memory = Arc::new(MemoryStore::default());
    memory.save(&refresh_token)?;
    let tokens = TokenProvider::new(oauth(&cli.tenant, &client_id), memory.clone());
    let address = backend_over(tokens)
        .address()
        .await
        .context("finding out which account the existing sign-in is")?
        .context("Microsoft did not say which account the existing sign-in is")?;
    // Asking may have rotated the refresh token.
    let refresh_token = memory.load()?.unwrap_or(refresh_token);
    KeyringStore::new(&address)?.save(&refresh_token)?;
    let old = data_dir()?.join("cache.db");
    let new = cache_path(&address)?;
    for suffix in ["", "-wal", "-shm"] {
        let mut from = old.as_os_str().to_owned();
        from.push(suffix);
        let mut to = new.as_os_str().to_owned();
        to.push(suffix);
        match std::fs::rename(&from, &to) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(Accounts(vec![Account {
        address,
        tenant: cli.tenant.clone(),
        client_id,
        signature: String::new(),
    }]))
}

async fn sync_once(cli: &Cli) -> Result<()> {
    let accounts = signed_in(cli).await?;
    if accounts.0.is_empty() {
        bail!("not signed in; run `otlk login` first");
    }
    for account in &accounts.0 {
        let backend = graph_backend(account)?;
        let mut store = Store::open(&cache_path(&account.address)?)?;
        let address = &account.address;
        sync_folders(&backend, &mut store)
            .await
            .with_context(|| format!("syncing {address}"))?;
        for folder in store.folders()? {
            sync_messages(&backend, &mut store, &folder.id)
                .await
                .with_context(|| format!("syncing {address} / {}", folder.name))?;
            let count = store.messages_in(&folder.id)?.len();
            println!("{address} / {}: {count} messages", folder.name);
        }
    }
    Ok(())
}

/// How often to check for new mail, or never when it is 0.
fn sync_every(cli: &Cli) -> Option<Duration> {
    (cli.sync_every > 0).then(|| Duration::from_secs(cli.sync_every))
}

fn data_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "tuilook")
        .context("no home directory to keep the mail cache in")?;
    std::fs::create_dir_all(dirs.data_dir())?;
    Ok(dirs.data_dir().to_owned())
}

fn accounts_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("accounts.json"))
}

/// Where an account's cached mail is kept.
fn account_dir(address: &str) -> Result<PathBuf> {
    Ok(data_dir()?.join(address))
}

fn cache_path(address: &str) -> Result<PathBuf> {
    let dir = account_dir(address)?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("cache.db"))
}

/// Where the place the user was when they quit is kept.
fn session_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "tuilook")
        .context("no home directory to keep the session in")?;
    std::fs::create_dir_all(dirs.cache_dir())?;
    Ok(dirs.cache_dir().join("session.json"))
}

/// Deletes a SQLite database along with its WAL files.
fn remove_db(path: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

/// Renders from the caches at once while core syncs every account in the
/// background. `accounts` holds each account's address, backend and cache.
/// Their signatures are kept in `accounts_file`; without one, as in the
/// demo, signatures last until tuilook closes.
fn run_ui<B: MailBackend + Send + Sync + 'static>(
    runtime: &tokio::runtime::Runtime,
    accounts: Vec<(String, B, PathBuf)>,
    accounts_file: Option<&Path>,
    session: Option<&Path>,
    sync_every: Option<Duration>,
) -> Result<()> {
    let mut app = AppState::default();
    let saved = match accounts_file {
        Some(path) => Accounts::load(path)?,
        None => Accounts::default(),
    };
    let mut mailboxes = Vec::new();
    let mut events = Vec::new();
    for (address, backend, path) in accounts {
        let (commands, command_rx) = mpsc::channel(32);
        let (event_tx, event_rx) = mpsc::channel(32);
        runtime.spawn(run_engine(
            backend,
            Store::open(&path)?,
            command_rx,
            event_tx,
        ));
        let store = Store::open(&path)?;
        let signature = saved.find(&address).map(|a| a.signature.clone());
        app.signatures.push(signature.unwrap_or_default());
        app.accounts.push(address);
        mailboxes.push(Mailbox { store, commands });
        events.push(event_rx);
    }
    let accounts_file = accounts_file.map(Path::to_path_buf);
    let mut mailboxes = Mailboxes::new(mailboxes, accounts_file, Box::new(io::stdout()));
    app.sync.every = sync_every;
    let effect = app.start_sync();
    mailboxes.apply(&mut app, effect)?;
    for account in 0..mailboxes.len() {
        let effect = app.set_account_folders(account, mailboxes.store(account).folders()?);
        mailboxes.apply(&mut app, effect)?;
    }
    if let Some(snapshot) = session.and_then(session::load) {
        let effect = app.restore(&snapshot);
        mailboxes.apply(&mut app, effect)?;
        let effect = snapshot
            .message
            .as_deref()
            .and_then(|id| app.select_message(id));
        mailboxes.apply(&mut app, effect)?;
    }

    app.clock = Some(Zoned::now());
    let mut keymap = Keymap::default();
    let mut terminal = ratatui::init();
    let mut cursor = Cursor::Default;
    let result = (|| {
        execute!(io::stdout(), EnableMouseCapture)?;
        while !app.should_quit {
            let now = Instant::now();
            app.expire_status(now);
            let effect = app.sync_if_due(now);
            mailboxes.apply(&mut app, effect)?;
            terminal.draw(|frame| ui::draw(frame, &app))?;
            let shape = Cursor::of(&app);
            if shape != cursor {
                execute!(io::stdout(), shape.style())?;
                cursor = shape;
            }
            for (account, events) in events.iter_mut().enumerate() {
                while let Ok(event) = events.try_recv() {
                    mailboxes.on_event(&mut app, account, event)?;
                }
            }
            let ready = event::poll(Duration::from_millis(100))?;
            // The key and the drawing after it see the same time.
            app.clock = Some(Zoned::now());
            if !ready {
                continue;
            }
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            app.viewport = ui::viewport(area, &app);
            let effect = match event::read()? {
                // Windows also reports key releases.
                TermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    app.on_key(key, &mut keymap)
                }
                TermEvent::Mouse(mouse) => {
                    let action = match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left) => Mouse::Click,
                        MouseEventKind::ScrollUp => Mouse::ScrollUp,
                        MouseEventKind::ScrollDown => Mouse::ScrollDown,
                        _ => continue,
                    };
                    let hit = ui::hit(area, &app, mouse.column, mouse.row);
                    app.on_mouse(action, hit)
                }
                _ => continue,
            };
            mailboxes.apply(&mut app, effect)?;
        }
        Ok(())
    })();
    let _ = execute!(io::stdout(), DisableMouseCapture);
    if let Some(path) = session {
        // Losing the place to start from next time is not worth failing over.
        let _ = session::save(path, &session::Snapshot::of(&app));
    }
    if cursor != Cursor::Default {
        let _ = execute!(io::stdout(), Cursor::Default.style());
    }
    ratatui::restore();
    result
}

/// The cursor's shape: a bar while typing, a block in vim's other modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cursor {
    Default,
    Bar,
    Block,
}

impl Cursor {
    fn of(app: &AppState) -> Self {
        if app.dialog.as_ref().is_some_and(Dialog::takes_text) {
            return Cursor::Bar;
        }
        match &app.compose {
            None if app.typing_search() => Cursor::Bar,
            None => match &app.signature {
                Some(editor) if editor.body.vim.mode == VimMode::Insert => Cursor::Bar,
                Some(_) => Cursor::Block,
                None => Cursor::Default,
            },
            Some(compose) if compose.field != Field::Body => Cursor::Bar,
            Some(compose) if compose.body.vim.mode == VimMode::Insert => Cursor::Bar,
            Some(_) => Cursor::Block,
        }
    }

    fn style(self) -> SetCursorStyle {
        match self {
            Cursor::Default => SetCursorStyle::DefaultUserShape,
            Cursor::Bar => SetCursorStyle::SteadyBar,
            Cursor::Block => SetCursorStyle::SteadyBlock,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_in_with_the_tuilook_app_registration_by_default() {
        let cli = Cli::try_parse_from(["otlk"]).unwrap();

        assert_eq!(
            cli.client_id.as_deref(),
            Some("5de297ab-80a5-4cfc-bdea-38e33b917993")
        );
    }

    #[test]
    fn signs_in_to_personal_accounts_by_default() {
        // The default app registration accepts personal Microsoft accounts only.
        let cli = Cli::try_parse_from(["otlk"]).unwrap();

        assert_eq!(cli.tenant, "consumers");
    }

    #[test]
    fn the_client_id_can_be_overridden() {
        let cli = Cli::try_parse_from(["otlk", "--client-id", "my-own-app"]).unwrap();

        assert_eq!(cli.client_id.as_deref(), Some("my-own-app"));
    }

    #[test]
    fn checks_for_new_mail_every_minute_unless_told_otherwise() {
        let cli = Cli::try_parse_from(["otlk"]).unwrap();
        assert_eq!(sync_every(&cli), Some(Duration::from_secs(60)));

        let cli = Cli::try_parse_from(["otlk", "--sync-every", "0"]).unwrap();
        assert_eq!(sync_every(&cli), None);
    }
}
