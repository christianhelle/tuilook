use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
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
use tuilook_auth::{KeyringStore, OAuthClient, OAuthConfig, SecretStore, TokenProvider};
use tuilook_core::{Command, run_engine, sync_folders, sync_messages};
use tuilook_graph::{GRAPH_V1, GraphBackend, GraphClient};
use tuilook_model::{Event, MailBackend};
use tuilook_store::Store;
use tuilook_tui::{
    app::{AppState, Effect, Mouse},
    compose::Field,
    demo,
    keymap::Keymap,
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

/// Keychain account the refresh token is stored under.
const ACCOUNT: &str = "default";

/// tuilook's own Entra ID app registration (a public client; not a secret).
const DEFAULT_CLIENT_ID: &str = "5de297ab-80a5-4cfc-bdea-38e33b917993";

#[derive(Parser)]
#[command(version, about = "A terminal mail client for Microsoft 365")]
struct Cli {
    /// Entra ID application (client) ID; use your own if your tenant blocks tuilook's.
    #[arg(long, env = "TUILOOK_CLIENT_ID", default_value = DEFAULT_CLIENT_ID, global = true)]
    client_id: Option<String>,
    /// Tenant to sign in to: consumers (personal accounts), organizations, common
    /// or a tenant ID. The default app registration only accepts consumers.
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
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Sign in with a device code.
    Login,
    /// Forget the stored sign-in.
    Logout,
    /// Sync all folders once, without the UI.
    Sync,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let runtime = tokio::runtime::Runtime::new()?;
    match cli.command {
        Some(CliCommand::Login) => runtime.block_on(login(&cli)),
        Some(CliCommand::Logout) => {
            KeyringStore::new(ACCOUNT)?.clear()?;
            println!("Signed out.");
            Ok(())
        }
        Some(CliCommand::Sync) => runtime.block_on(sync_once(&cli)),
        None if cli.demo => {
            let path = std::env::temp_dir().join("tuilook-demo.db");
            remove_db(&path)?;
            run_ui(&runtime, demo::backend(), &path, None)
        }
        None => {
            let backend = graph_backend(&cli)?;
            if KeyringStore::new(ACCOUNT)?.load()?.is_none() {
                bail!("not signed in; run `tuilook login` first");
            }
            run_ui(&runtime, backend, &cache_path()?, Some(&session_path()?))
        }
    }
}

fn token_provider(cli: &Cli) -> Result<TokenProvider> {
    let Some(client_id) = cli.client_id.clone().filter(|id| !id.is_empty()) else {
        bail!(
            "no client ID: register a public client app in Entra ID, then pass \
             --client-id or set TUILOOK_CLIENT_ID"
        );
    };
    let oauth = OAuthClient::new(OAuthConfig {
        authority: "https://login.microsoftonline.com".to_owned(),
        tenant: cli.tenant.clone(),
        client_id,
        scopes: SCOPES.map(str::to_owned).to_vec(),
    });
    Ok(TokenProvider::new(
        oauth,
        Arc::new(KeyringStore::new(ACCOUNT)?),
    ))
}

fn graph_backend(cli: &Cli) -> Result<GraphBackend<Arc<TokenProvider>>> {
    let tokens = Arc::new(token_provider(cli)?);
    Ok(GraphBackend::new(GraphClient::new(GRAPH_V1, tokens)))
}

async fn login(cli: &Cli) -> Result<()> {
    token_provider(cli)?
        .login(|code| println!("{}", code.message))
        .await?;
    println!("Signed in.");
    Ok(())
}

async fn sync_once(cli: &Cli) -> Result<()> {
    let backend = graph_backend(cli)?;
    let mut store = Store::open(&cache_path()?)?;
    sync_folders(&backend, &mut store).await?;
    for folder in store.folders()? {
        sync_messages(&backend, &mut store, &folder.id)
            .await
            .with_context(|| format!("syncing {}", folder.name))?;
        let count = store.messages_in(&folder.id)?.len();
        println!("{}: {count} messages", folder.name);
    }
    Ok(())
}

fn cache_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "tuilook")
        .context("no home directory to keep the mail cache in")?;
    std::fs::create_dir_all(dirs.data_dir())?;
    Ok(dirs.data_dir().join("cache.db"))
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

/// Renders from the cache at once while core syncs in the background.
fn run_ui(
    runtime: &tokio::runtime::Runtime,
    backend: impl MailBackend + Send + Sync + 'static,
    path: &Path,
    session: Option<&Path>,
) -> Result<()> {
    let store = Store::open(path)?;
    let (commands, command_rx) = mpsc::channel(32);
    let (event_tx, mut events) = mpsc::channel(32);
    runtime.spawn(run_engine(
        backend,
        Store::open(path)?,
        command_rx,
        event_tx,
    ));
    commands.blocking_send(Command::SyncAll)?;

    let mut app = AppState::default();
    let effect = app.set_folders(store.folders()?);
    apply(&mut app, &store, &commands, effect)?;
    if let Some(snapshot) = session.and_then(session::load) {
        let effect = app.restore(&snapshot);
        apply(&mut app, &store, &commands, effect)?;
        let effect = snapshot
            .message
            .as_deref()
            .and_then(|id| app.select_message(id));
        apply(&mut app, &store, &commands, effect)?;
    }

    let mut keymap = Keymap::default();
    let mut terminal = ratatui::init();
    let mut cursor = Cursor::Default;
    let result = (|| {
        execute!(io::stdout(), EnableMouseCapture)?;
        while !app.should_quit {
            terminal.draw(|frame| ui::draw(frame, &app))?;
            let shape = Cursor::of(&app);
            if shape != cursor {
                execute!(io::stdout(), shape.style())?;
                cursor = shape;
            }
            while let Ok(event) = events.try_recv() {
                let effect = on_core_event(&mut app, &store, event)?;
                apply(&mut app, &store, &commands, effect)?;
            }
            if !event::poll(Duration::from_millis(100))? {
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
            apply(&mut app, &store, &commands, effect)?;
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
        match &app.compose {
            None => Cursor::Default,
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

fn on_core_event(app: &mut AppState, store: &Store, event: Event) -> Result<Option<Effect>> {
    Ok(match event {
        Event::FoldersSynced => app.set_folders(store.folders()?),
        Event::FolderSynced(id) => {
            // Unread counts may have changed along with the messages.
            let effect = app.set_folders(store.folders()?);
            if app.current_folder() == Some(&id) {
                app.refresh_messages(store.messages_in(&id)?).or(effect)
            } else {
                effect
            }
        }
        Event::BodyLoaded { message, text } => {
            app.body_loaded(message, text);
            None
        }
        Event::MessagesDeleted(ids) => app.messages_deleted(&ids),
        Event::Sent => {
            app.mail_sent();
            None
        }
        Event::SendFailed(error) => {
            app.send_failed(&error);
            None
        }
        Event::Error(error) => {
            app.show_error(error);
            None
        }
    })
}

/// Performs the IO an update asked for, and any it leads to.
fn apply(
    app: &mut AppState,
    store: &Store,
    commands: &mpsc::Sender<Command>,
    mut effect: Option<Effect>,
) -> Result<()> {
    while let Some(next) = effect {
        effect = match next {
            Effect::LoadFolder(id) => app.set_messages(store.messages_in(&id)?),
            Effect::LoadBody(id) => {
                commands.blocking_send(Command::LoadBody(id))?;
                None
            }
            Effect::Sync => {
                commands.blocking_send(Command::SyncAll)?;
                None
            }
            Effect::Send(draft) => {
                commands.blocking_send(Command::Send(draft))?;
                None
            }
            Effect::Delete { ids, permanent } => {
                commands.blocking_send(Command::Delete { ids, permanent })?;
                None
            }
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_in_with_the_tuilook_app_registration_by_default() {
        let cli = Cli::try_parse_from(["tuilook"]).unwrap();

        assert_eq!(
            cli.client_id.as_deref(),
            Some("5de297ab-80a5-4cfc-bdea-38e33b917993")
        );
    }

    #[test]
    fn signs_in_to_personal_accounts_by_default() {
        // The default app registration accepts personal Microsoft accounts only.
        let cli = Cli::try_parse_from(["tuilook"]).unwrap();

        assert_eq!(cli.tenant, "consumers");
    }

    #[test]
    fn the_client_id_can_be_overridden() {
        let cli = Cli::try_parse_from(["tuilook", "--client-id", "my-own-app"]).unwrap();

        assert_eq!(cli.client_id.as_deref(), Some("my-own-app"));
    }
}
