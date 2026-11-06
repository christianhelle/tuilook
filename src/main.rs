use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use jiff::{Timestamp, Zoned};
use ratatui::{
    DefaultTerminal,
    crossterm::{
        cursor::SetCursorStyle,
        event::{self, DisableMouseCapture, EnableMouseCapture},
        execute,
    },
    layout::Rect,
};
use semver::Version;
use tokio::sync::mpsc;
use tuilook::accounts::{Account, Accounts};
use tuilook::auth::{
    DeviceCode, KeyringStore, MemoryStore, OAuthClient, OAuthConfig, SecretStore, TokenProvider,
    http_client,
};
use tuilook::core::{run_engine, sync_folders, sync_messages};
use tuilook::graph::{GRAPH_V1, GraphBackend, GraphClient};
use tuilook::model::MailBackend;
use tuilook::store::Store;
use tuilook::tui::{
    account_manager::{AccountRequest, Login, Reload},
    app::AppState,
    demo,
    keymap::Keymap,
    mailboxes::{Mailbox, Mailboxes},
    run_loop::{
        AccountNews, UpdateNews, on_terminal_event, take_account_news, take_core_events,
        take_update,
    },
    screen::Cursor,
    session, ui,
};
use tuilook::update;

/// Delegated Graph scopes; contacts can be added later.
const SCOPES: [&str; 6] = [
    "offline_access",
    "User.Read",
    "Mail.ReadWrite",
    "Mail.Send",
    "MailboxSettings.Read",
    "Calendars.ReadWrite",
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
    /// Don't ask GitHub for a newer tuilook when the mail client starts.
    #[arg(
        long,
        env = "TUILOOK_NO_UPDATE_CHECK",
        value_parser = clap::builder::FalseyValueParser::new()
    )]
    no_update_check: bool,
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
    /// Ask GitHub whether a newer tuilook has been released.
    CheckUpdate,
    /// Replace this otlk with the latest release.
    Update,
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
        Some(CliCommand::CheckUpdate) => runtime.block_on(check_update()),
        Some(CliCommand::Update) => runtime.block_on(update_otlk()),
        None if cli.demo => {
            let mut mailboxes = Vec::new();
            for (index, (address, backend)) in demo::accounts().into_iter().enumerate() {
                let path = std::env::temp_dir().join(format!("tuilook-demo-{index}.db"));
                remove_db(&path)?;
                mailboxes.push((address, backend, path));
            }
            // The demo stays offline: nothing ever sends on this channel.
            let (_, updates) = mpsc::channel(1);
            let mut services = Services::new(updates, default_login(&cli));
            let ui = Ui {
                accounts_file: None,
                session: None,
                sync_every: sync_every(&cli),
            };
            // Accounts are not changed in the demo, so it never reloads.
            with_terminal(|terminal| {
                run_ui(&runtime, terminal, mailboxes, &ui, &mut services).map(|_| ())
            })
        }
        None => {
            let accounts = runtime.block_on(signed_in(&cli))?;
            if accounts.0.is_empty() {
                bail!("not signed in; run `otlk login` first");
            }
            let (news, updates) = mpsc::channel(4);
            watch_for_update(&runtime, &cli, news.clone());
            let (said, account_news) = mpsc::unbounded_channel();
            let mut services = Services {
                installer: Some(install_on_request(&runtime, news)),
                signer: Some(sign_in_on_request(&runtime, said)),
                account_news: Some(account_news),
                ..Services::new(updates, default_login(&cli))
            };
            let ui = Ui {
                accounts_file: Some(accounts_path()?),
                session: Some(session_path()?),
                sync_every: sync_every(&cli),
            };
            let signed_out = with_terminal(|terminal| {
                loop {
                    let accounts = Accounts::load(&accounts_path()?)?;
                    if accounts.0.is_empty() {
                        return Ok(true);
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
                    match run_ui(&runtime, terminal, mailboxes, &ui, &mut services)? {
                        None => return Ok(false),
                        Some(Reload::Keep) => {}
                        Some(Reload::Remove(address)) => forget(&address)?,
                    }
                }
            })?;
            if signed_out {
                println!("Signed out of every account; run `otlk login` to sign in again.");
            }
            Ok(())
        }
    }
}

/// What a new account signs in with unless changed in the UI.
fn default_login(cli: &Cli) -> Login {
    Login {
        tenant: cli.tenant.clone(),
        client_id: cli.client_id.clone().unwrap_or_default(),
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

/// Signs in from the command line.
async fn login(cli: &Cli) -> Result<()> {
    // Moves an older tuilook's sign-in first, so it is not lost.
    signed_in(cli).await?;
    let address = sign_in(&cli.tenant, &client_id(cli)?, None, |code| {
        println!("{}", code.message)
    })
    .await?;
    println!("Signed in as {address}.");
    Ok(())
}

/// Signs in with a device code shown by `show_code`, keeping the refresh
/// token in memory until Graph says whose it is, then saves the account
/// and returns its address. With `expected`, signing in as anyone else
/// changes nothing.
async fn sign_in(
    tenant: &str,
    client_id: &str,
    expected: Option<&str>,
    show_code: impl FnOnce(&DeviceCode),
) -> Result<String> {
    let memory = Arc::new(MemoryStore::default());
    let tokens = TokenProvider::new(oauth(tenant, client_id), memory.clone());
    tokens.login(show_code).await?;
    let address = backend_over(tokens)
        .address()
        .await?
        .context("Microsoft did not say which account signed in")?;
    if let Some(expected) = expected.filter(|e| !e.eq_ignore_ascii_case(&address)) {
        bail!("signed in as {address}, not {expected}");
    }
    let refresh_token = memory
        .load()?
        .context("Microsoft did not let tuilook stay signed in (no refresh token)")?;
    KeyringStore::new(&address)?.save(&refresh_token)?;
    let path = accounts_path()?;
    let mut accounts = Accounts::load(&path)?;
    accounts.upsert(Account {
        address: address.clone(),
        tenant: tenant.to_owned(),
        client_id: client_id.to_owned(),
        signature: String::new(),
    });
    accounts.save(&path)?;
    Ok(address)
}

async fn logout(cli: &Cli, query: Option<&str>) -> Result<()> {
    let accounts = signed_in(cli).await?;
    let account = match (query, &accounts.0[..]) {
        (Some(query), _) => accounts.find(query)?.clone(),
        (None, [only]) => only.clone(),
        (None, []) => {
            println!("Not signed in.");
            return Ok(());
        }
        (None, _) => bail!("signed in to several accounts; say which: `otlk logout <address>`"),
    };
    forget(&account.address)?;
    println!("Signed out of {}.", account.address);
    Ok(())
}

/// Forgets the account at `address`: its sign-in, its place in the list and
/// its cached mail. Its cache must be closed.
fn forget(address: &str) -> Result<()> {
    KeyringStore::new(address)?.clear()?;
    let path = accounts_path()?;
    let mut accounts = Accounts::load(&path)?;
    accounts.remove(address);
    accounts.save(&path)?;
    match std::fs::remove_dir_all(account_dir(address)?) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
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

fn cache_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "tuilook")
        .context("no home directory to keep tuilook's cache in")?;
    std::fs::create_dir_all(dirs.cache_dir())?;
    Ok(dirs.cache_dir().to_owned())
}

/// Where the place the user was when they quit is kept.
fn session_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("session.json"))
}

/// Where the last check for a newer tuilook is kept.
fn update_check_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("update-check.json"))
}

/// Looks for a newer tuilook in the background, unless told not to, and
/// tells `updates` if there is one.
fn watch_for_update(
    runtime: &tokio::runtime::Runtime,
    cli: &Cli,
    updates: mpsc::Sender<UpdateNews>,
) {
    if cli.no_update_check {
        return;
    }
    // With nowhere to remember checks, tuilook would ask GitHub on every start.
    let Ok(path) = update_check_path() else {
        return;
    };
    runtime.spawn(async move {
        let update = update::available(
            &http_client(),
            update::GITHUB_API,
            &path,
            &update::current(),
            Timestamp::now(),
        )
        .await;
        if let Some(version) = update {
            let _ = updates.send(UpdateNews::Available(version)).await;
        }
    });
}

/// Replaces the running otlk with the latest release in the background
/// each time the returned sender asks, and tells `news` how it went.
fn install_on_request(
    runtime: &tokio::runtime::Runtime,
    news: mpsc::Sender<UpdateNews>,
) -> mpsc::UnboundedSender<()> {
    let (installer, mut asked) = mpsc::unbounded_channel();
    runtime.spawn(async move {
        while asked.recv().await.is_some() {
            let outcome = match update_check_path() {
                Ok(path) => update::update_running(&http_client(), update::GITHUB_API, &path).await,
                Err(e) => Err(e),
            };
            let said = match outcome {
                Ok(update::Outcome::Installed(version)) => UpdateNews::Installed(version),
                Ok(update::Outcome::UpToDate(_)) => UpdateNews::UpToDate,
                Err(e) => UpdateNews::Failed(format!("{e:#}")),
            };
            if news.send(said).await.is_err() {
                break;
            }
        }
    });
    installer
}

/// Signs accounts in in the background each time the returned sender asks,
/// and tells `news` how it went. A new request stops the one under way.
fn sign_in_on_request(
    runtime: &tokio::runtime::Runtime,
    news: mpsc::UnboundedSender<AccountNews>,
) -> mpsc::UnboundedSender<AccountRequest> {
    let (signer, mut asked) = mpsc::unbounded_channel();
    runtime.spawn(async move {
        let mut running: Option<tokio::task::JoinHandle<()>> = None;
        while let Some(request) = asked.recv().await {
            if let Some(task) = running.take() {
                task.abort();
            }
            let AccountRequest::SignIn { login, replacing } = request else {
                continue;
            };
            let news = news.clone();
            running = Some(tokio::spawn(async move {
                let codes = news.clone();
                let signed = sign_in(
                    &login.tenant,
                    &login.client_id,
                    replacing.as_deref(),
                    |code| {
                        let _ = codes.send(AccountNews::Code(code.message.clone()));
                    },
                )
                .await;
                let said = match signed {
                    Ok(address) => AccountNews::SignedIn { address, login },
                    Err(e) => AccountNews::Failed(format!("{e:#}")),
                };
                let _ = news.send(said);
            }));
        }
    });
    signer
}

/// Tells the user whether a newer tuilook has been released.
async fn check_update() -> Result<()> {
    let release = update::check(
        &http_client(),
        update::GITHUB_API,
        &update_check_path()?,
        Timestamp::now(),
    )
    .await
    .context("could not check for updates")?;
    let current = update::current();
    if update::newer(&current, &release.version) {
        println!(
            "tuilook {} is available (you have {current}): {}",
            release.version, release.url
        );
    } else {
        println!("tuilook {current} is up to date.");
    }
    Ok(())
}

/// Replaces this otlk with the latest release.
async fn update_otlk() -> Result<()> {
    let outcome = update::update_running(&http_client(), update::GITHUB_API, &update_check_path()?)
        .await
        .context("could not update")?;
    match outcome {
        update::Outcome::UpToDate(current) => println!("tuilook {current} is up to date."),
        update::Outcome::Installed(version) => println!(
            "Updated tuilook {} to {version}; it runs from the next start.",
            update::current()
        ),
    }
    Ok(())
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

/// Where the UI keeps what lasts beyond it, and how often it syncs.
struct Ui {
    /// Where accounts and their signatures are kept; without one, as in the
    /// demo, signatures last until tuilook closes.
    accounts_file: Option<PathBuf>,
    /// Where the place the user was is kept between starts.
    session: Option<PathBuf>,
    sync_every: Option<Duration>,
}

/// What outlasts the UI when it starts again for accounts to change.
struct Services {
    updates: mpsc::Receiver<UpdateNews>,
    installer: Option<mpsc::UnboundedSender<()>>,
    signer: Option<mpsc::UnboundedSender<AccountRequest>>,
    account_news: Option<mpsc::UnboundedReceiver<AccountNews>>,
    default_login: Login,
    /// A newer release found, and one installed, so far.
    update: Option<Version>,
    installed: Option<Version>,
}

impl Services {
    fn new(updates: mpsc::Receiver<UpdateNews>, default_login: Login) -> Self {
        Self {
            updates,
            installer: None,
            signer: None,
            account_news: None,
            default_login,
            update: None,
            installed: None,
        }
    }
}

/// Runs `f` with the terminal taken over for the UI, and gives it back.
fn with_terminal<T>(f: impl FnOnce(&mut DefaultTerminal) -> Result<T>) -> Result<T> {
    let mut terminal = ratatui::init();
    let result = execute!(io::stdout(), EnableMouseCapture)
        .map_err(anyhow::Error::from)
        .and_then(|()| f(&mut terminal));
    let _ = execute!(io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

/// Renders from the caches at once while core syncs every account in the
/// background. `accounts` holds each account's address, backend and cache.
/// Returns how to start again when the UI closed for accounts to change.
fn run_ui<B: MailBackend + Send + Sync + 'static>(
    runtime: &tokio::runtime::Runtime,
    terminal: &mut DefaultTerminal,
    accounts: Vec<(String, B, PathBuf)>,
    ui: &Ui,
    services: &mut Services,
) -> Result<Option<Reload>> {
    let mut app = AppState {
        default_login: services.default_login.clone(),
        update: services.update.clone(),
        installed: services.installed.clone(),
        ..AppState::default()
    };
    let saved = match &ui.accounts_file {
        Some(path) => Accounts::load(path)?,
        None => Accounts::default(),
    };
    let mut mailboxes = Vec::new();
    let mut events = Vec::new();
    let mut engines = Vec::new();
    for (address, backend, path) in accounts {
        let (commands, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel(32);
        engines.push(runtime.spawn(run_engine(
            backend,
            Store::open(&path)?,
            command_rx,
            event_tx,
        )));
        let store = Store::open(&path)?;
        let account = saved.find(&address).ok();
        app.signatures
            .push(account.map(|a| a.signature.clone()).unwrap_or_default());
        app.logins.push(account.map_or_else(
            || services.default_login.clone(),
            |a| Login {
                tenant: a.tenant.clone(),
                client_id: a.client_id.clone(),
            },
        ));
        app.accounts.push(address);
        mailboxes.push(Mailbox { store, commands });
        events.push(event_rx);
    }
    let mut mailboxes = Mailboxes::new(mailboxes, ui.accounts_file.clone(), Box::new(io::stdout()));
    if let Some(installer) = services.installer.clone() {
        mailboxes = mailboxes.with_installer(installer);
    }
    if let Some(signer) = services.signer.clone() {
        mailboxes = mailboxes.with_signer(signer);
    }
    let result = drive(
        &mut app,
        &mut mailboxes,
        &mut events,
        terminal,
        ui,
        services,
    );
    if let Some(path) = &ui.session {
        // Losing the place to start from next time is not worth failing over.
        let _ = session::save(path, &session::Snapshot::of(&app));
    }
    services.update = app.update.clone();
    services.installed = app.installed.clone();
    // Each cache must be closed before an account's can be deleted.
    drop(mailboxes);
    for engine in &engines {
        engine.abort();
    }
    for engine in engines {
        let _ = runtime.block_on(engine);
    }
    result.map(|()| app.reload.take())
}

/// Starts syncing, shows where the user was, then draws and takes keys until
/// the app quits.
fn drive(
    app: &mut AppState,
    mailboxes: &mut Mailboxes,
    events: &mut [mpsc::Receiver<tuilook::model::Event>],
    terminal: &mut DefaultTerminal,
    ui: &Ui,
    services: &mut Services,
) -> Result<()> {
    app.sync.every = ui.sync_every;
    let effect = app.start_sync();
    mailboxes.apply(app, effect)?;
    for account in 0..mailboxes.len() {
        let effect = app.set_account_folders(account, mailboxes.store(account).folders()?);
        mailboxes.apply(app, effect)?;
    }
    if let Some(snapshot) = ui.session.as_deref().and_then(session::load) {
        let effect = app.restore(&snapshot);
        mailboxes.apply(app, effect)?;
        let effect = snapshot
            .message
            .as_deref()
            .and_then(|id| app.select_message(id));
        mailboxes.apply(app, effect)?;
    }

    app.clock = Some(Zoned::now());
    let mut keymap = Keymap::default();
    let mut cursor = Cursor::Default;
    let result = (|| {
        while !app.should_quit {
            let now = Instant::now();
            app.expire_status(now);
            let effect = app.sync_if_due(now);
            mailboxes.apply(app, effect)?;
            terminal.draw(|frame| ui::draw(frame, app))?;
            let shape = app.cursor();
            if shape != cursor {
                execute!(io::stdout(), cursor_style(shape))?;
                cursor = shape;
            }
            take_core_events(app, mailboxes, events)?;
            take_update(app, &mut services.updates);
            if let Some(news) = &mut services.account_news {
                take_account_news(app, news);
            }
            let ready = event::poll(Duration::from_millis(100))?;
            // The key and the drawing after it see the same time.
            app.clock = Some(Zoned::now());
            if !ready {
                continue;
            }
            let size = terminal.size()?;
            let area = Rect::new(0, 0, size.width, size.height);
            on_terminal_event(app, mailboxes, &mut keymap, area, event::read()?)?;
        }
        Ok(())
    })();
    if cursor != Cursor::Default {
        let _ = execute!(io::stdout(), cursor_style(Cursor::Default));
    }
    result
}

/// How the terminal draws `cursor`.
fn cursor_style(cursor: Cursor) -> SetCursorStyle {
    match cursor {
        Cursor::Default => SetCursorStyle::DefaultUserShape,
        Cursor::Bar => SetCursorStyle::SteadyBar,
        Cursor::Block => SetCursorStyle::SteadyBlock,
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

    #[test]
    fn the_update_check_can_be_turned_off() {
        let cli = Cli::try_parse_from(["otlk", "--no-update-check"]).unwrap();

        assert!(cli.no_update_check);
    }

    #[test]
    fn update_is_a_command() {
        let cli = Cli::try_parse_from(["otlk", "update"]).unwrap();

        assert!(matches!(cli.command, Some(CliCommand::Update)));
    }

    #[test]
    fn check_update_is_a_command() {
        let cli = Cli::try_parse_from(["otlk", "check-update"]).unwrap();

        assert!(matches!(cli.command, Some(CliCommand::CheckUpdate)));
    }
}
