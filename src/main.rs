use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod app;
mod db;
mod find;
mod icons;
mod ops;
mod panel;
mod preview;
mod tree;
mod ui;

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

        if let Some(cmd) = app.pending_shell.take() {
            run_shell(terminal, app, &cmd)?;
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn run_shell(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
    cmd: &str,
) -> io::Result<()> {
    let cwd = app.active_panel().path.clone();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());

    if cmd.is_empty() {
        let _ = std::process::Command::new(&shell)
            .current_dir(&cwd)
            .status();
    } else {
        let _ = std::process::Command::new(&shell)
            .arg("-c")
            .arg(cmd)
            .current_dir(&cwd)
            .status();
        eprintln!("\n[Press Enter to continue]");
        let _ = io::stdin().read_line(&mut String::new());
    }

    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;

    let tab = app.tab_mut();
    let _ = tab.left.load_dir();
    let _ = tab.right.load_dir();

    Ok(())
}
