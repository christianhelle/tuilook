use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use tuilook_core::{sync_folders, sync_messages};
use tuilook_store::Store;
use tuilook_tui::{
    app::{AppState, Effect},
    demo, keymap, ui,
};

fn main() -> Result<()> {
    let store = load_demo_store()?;
    let mut app = AppState {
        folders: store.folders()?,
        ..Default::default()
    };
    if let Some(first) = app.folders.first() {
        let _ = app.set_messages(store.messages_in(&first.id)?);
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &store);
    ratatui::restore();
    result
}

fn load_demo_store() -> Result<Store> {
    let backend = demo::backend();
    let mut store = Store::open_in_memory()?;
    tokio::runtime::Builder::new_current_thread()
        .build()?
        .block_on(async {
            sync_folders(&backend, &mut store).await?;
            for folder in store.folders()? {
                sync_messages(&backend, &mut store, &folder.id).await?;
            }
            anyhow::Ok(())
        })?;
    Ok(store)
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut AppState, store: &Store) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, app))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let Some(action) = keymap::action_for(key) else {
            continue;
        };
        let mut effect = app.update(action);
        while let Some(next) = effect {
            effect = match next {
                Effect::LoadFolder(id) => app.set_messages(store.messages_in(&id)?),
                Effect::LoadBody(_) => None,
            };
        }
    }
    Ok(())
}
