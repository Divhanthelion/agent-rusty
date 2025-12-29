use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use std::process::Stdio;
use std::time::Duration;
use tokio::sync::mpsc;

mod actions;
mod app;
mod skeleton;
mod tmux;

use actions::Action;
use app::App;
use tmux::TmuxClient;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<Action>();

    // Initialize terminal
    let mut terminal = ratatui::init();

    // Spawn input handler
    let input_tx = tx.clone();
    tokio::spawn(async move {
        loop {
            if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                if let Ok(evt) = event::read() {
                    if let Event::Key(key) = evt {
                        if key.kind == KeyEventKind::Press {
                            let _ = input_tx.send(Action::KeyPress(key));
                        }
                    }
                }
            }
        }
    });

    // Spawn tmux poller
    let tmux_tx = tx.clone();
    tokio::spawn(async move {
        let client = TmuxClient::new();
        loop {
            match client.list_sessions().await {
                Ok(sessions) => {
                    let _ = tmux_tx.send(Action::SessionsUpdated(sessions));
                }
                Err(e) => {
                    let _ = tmux_tx.send(Action::Error(format!("Tmux: {}", e)));
                }
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
    });

    // Create shared tmux client for actions
    let tmux_client = TmuxClient::new();

    // Create app state
    let mut app = App::new();

    // Main event loop
    let result = loop {
        // Render
        terminal.draw(|f| app.render(f))?;

        // Process any pending actions from the app
        for pending_action in app.take_pending_actions() {
            match pending_action {
                Action::AttachSession(ref session_id) => {
                    // Suspend TUI and attach to session
                    ratatui::restore();

                    let cmd = tmux_client.attach_command(session_id);
                    let status = std::process::Command::new(&cmd[0])
                        .args(&cmd[1..])
                        .stdin(Stdio::inherit())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit())
                        .status();

                    // Resume TUI
                    terminal = ratatui::init();

                    if let Err(e) = status {
                        app.error_message = Some(format!("Failed to attach: {}", e));
                    }
                }
                Action::CreateSession(ref name) => {
                    match tmux_client.create_session(name).await {
                        Ok(_) => {
                            app.error_message = Some(format!("Session '{}' created", name));
                        }
                        Err(e) => {
                            app.error_message = Some(format!("Failed to create: {}", e));
                        }
                    }
                }
                Action::DeleteSession(ref session_id) => {
                    match tmux_client.kill_session(session_id).await {
                        Ok(_) => {
                            app.error_message = Some("Session deleted".to_string());
                        }
                        Err(e) => {
                            app.error_message = Some(format!("Failed to delete: {}", e));
                        }
                    }
                }
                Action::CopySkeleton => {
                    match skeleton::generate_skeleton(".").await {
                        Ok(tree) => match arboard::Clipboard::new() {
                            Ok(mut clipboard) => {
                                if let Err(e) = clipboard.set_text(&tree) {
                                    app.error_message = Some(format!("Clipboard error: {}", e));
                                } else {
                                    app.error_message =
                                        Some("Skeleton copied to clipboard!".to_string());
                                }
                            }
                            Err(e) => {
                                app.error_message = Some(format!("Clipboard error: {}", e));
                            }
                        },
                        Err(e) => {
                            app.error_message = Some(format!("Skeleton error: {}", e));
                        }
                    }
                }
                _ => {}
            }
        }

        // Handle events from channel
        tokio::select! {
            Some(action) = rx.recv() => {
                match app.handle_action(action) {
                    Ok(should_quit) => {
                        if should_quit {
                            break Ok(());
                        }
                    }
                    Err(e) => {
                        break Err(e);
                    }
                }
            }
        }
    };

    // Restore terminal
    ratatui::restore();
    result
}
