use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use ratatui::crossterm::event::{self, Event as TermEvent, KeyEventKind};
use tokio::sync::mpsc;
use tuilook_auth::{KeyringStore, OAuthClient, OAuthConfig, SecretStore, TokenProvider};
use tuilook_core::{Command, run_engine, sync_folders, sync_messages};
use tuilook_graph::{GRAPH_V1, GraphBackend, GraphClient};
use tuilook_model::{Event, MailBackend};
use tuilook_store::Store;
use tuilook_tui::{
    app::{AppState, Effect},
    demo, keymap, ui,
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

#[derive(Parser)]
#[command(version, about = "A terminal mail client for Microsoft 365")]
struct Cli {
    /// Entra ID application (client) ID of your app registration.
    #[arg(long, env = "TUILOOK_CLIENT_ID", global = true)]
    client_id: Option<String>,
    /// Tenant to sign in to: common, organizations, consumers or a tenant ID.
    #[arg(long, env = "TUILOOK_TENANT", default_value = "common", global = true)]
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
            run_ui(&runtime, demo::backend(), &path)
        }
        None => {
            let backend = graph_backend(&cli)?;
            if KeyringStore::new(ACCOUNT)?.load()?.is_none() {
                bail!("not signed in; run `tuilook login` first");
            }
            run_ui(&runtime, backend, &cache_path()?)
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

    let mut terminal = ratatui::init();
    let result = (|| {
        while !app.should_quit {
            terminal.draw(|frame| ui::draw(frame, &app))?;
            while let Ok(event) = events.try_recv() {
                let effect = on_core_event(&mut app, &store, event)?;
                apply(&mut app, &store, &commands, effect)?;
            }
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            let TermEvent::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if let Some(action) = keymap::action_for(key) {
                let effect = app.update(action);
                apply(&mut app, &store, &commands, effect)?;
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
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
        };
    }
    Ok(())
}
