use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

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

fn main() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let mut app = app::App::new()?;
    let result = run(&mut terminal, &mut app);

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

    let result = std::process::Command::new(&editor)
        .arg(path)
        .status();

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

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key);
                }
                _ => {}
            }
        }

        app.poll_progress();
        app.poll_du();
        app.poll_find();

        if let Some(path) = app.open_editor.take() {
            open_in_editor(terminal, app, &path)?;
        }

        if app.should_quit {
            app.save_session();
            return Ok(());
        }
    }
}

