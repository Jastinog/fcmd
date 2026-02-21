use std::io;
use std::time::Duration;

use crossterm::{
    event::{Event, EventStream, KeyEventKind},
    execute,
    terminal::{
        self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod db;
mod find;
mod icons;
mod ops;
mod panel;
mod preview;
mod theme;
mod tree;
mod ui;
mod util;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Restore terminal on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
        default_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let mut app = app::App::new()?;
    let result = run(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
    }
    Ok(())
}

fn open_in_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
    path: &std::path::Path,
) -> io::Result<()> {
    // Suspend TUI: switch to main screen, then clear it
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        terminal::Clear(terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0),
        crossterm::cursor::Show,
    )?;

    // Determine editor: $VISUAL -> $EDITOR -> vi
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());

    let result = std::process::Command::new(&editor).arg(path).status();

    // Restore TUI: enter alternate screen, force full repaint
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        terminal::Clear(terminal::ClearType::All),
        crossterm::cursor::Hide,
    )?;
    enable_raw_mode()?;
    // Invalidate ratatui's internal buffer so next draw() repaints every cell
    terminal.clear()?;

    match result {
        Ok(status) if status.success() => {
            // Refresh panel in case the file was modified
            let _ = app.active_panel_mut().load_dir();
        }
        Ok(status) => {
            app.status_message = format!("{editor} exited with {status}");
        }
        Err(e) => {
            app.status_message = format!("Failed to open {editor}: {e}");
        }
    }

    Ok(())
}

/// Helper: await a value from an Option<oneshot::Receiver>, or pend forever if None.
async fn recv_or_pend<T>(rx: &mut Option<tokio::sync::oneshot::Receiver<T>>) -> T {
    match rx {
        Some(r) => {
            let val = r.await.unwrap();
            *rx = None;
            val
        }
        None => std::future::pending().await,
    }
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> io::Result<()> {
    let mut reader = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));

    loop {
        if app.force_redraw {
            terminal.clear()?;
            app.force_redraw = false;
        }
        terminal.draw(|f| ui::render(f, app))?;

        tokio::select! {
            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        app.handle_key(key);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {}
                    None => break,
                }
            }
            _ = tick.tick() => {
                app.poll_progress();
                app.poll_du();
                app.poll_find();
                app.poll_info_du();
                app.poll_git();
            }
            result = recv_or_pend(&mut app.dir_load_rx) => {
                app.apply_dir_load(result);
            }
            result = recv_or_pend(&mut app.preview_load_rx) => {
                app.apply_preview_load(result);
            }
            result = recv_or_pend(&mut app.file_preview_rx) => {
                app.apply_file_preview_load(result);
            }
        }

        if let Some(path) = app.open_editor.take() {
            open_in_editor(terminal, app, &path)?;
        }

        if app.should_quit {
            app.save_session();
            return Ok(());
        }
    }
    Ok(())
}
