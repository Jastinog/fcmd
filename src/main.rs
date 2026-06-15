use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind},
    execute,
    terminal::{
        self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod archive;
mod exe;
mod fs;
mod model;
mod preview;
mod search;
mod storage;
mod theme;
mod ui;
mod util;
mod viewer;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Restore terminal on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        default_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let mut app = app::App::new()?;
    let result = run(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
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
        DisableMouseCapture,
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
        EnableMouseCapture,
        terminal::Clear(terminal::ClearType::All),
        crossterm::cursor::Hide,
    )?;
    enable_raw_mode()?;
    // Invalidate ratatui's internal buffer so next draw() repaints every cell
    terminal.clear()?;

    match result {
        Ok(status) if status.success() => {
            // Refresh panel in case the file was modified
            app.reload_active_panel();
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

/// Lightweight snapshot of state that poll functions may change.
/// Used to detect whether a tick actually modified anything worth redrawing.
fn snapshot(
    app: &app::App,
) -> (
    String,
    Option<String>,
    Option<String>,
    bool,
    usize,
    usize,
    usize,
    usize,
) {
    let find_count = app.find_state.as_ref().map_or(0, |fs| fs.total_count());
    (
        app.status_message.clone(),
        app.task_notification.clone(),
        app.background_progress.clone(),
        app.conflict_info.is_some(),
        app.dir_sizes.len(),
        app.git_statuses.len(),
        app.info_lines.len(),
        find_count,
    )
}

/// Helper: await a value from an Option<oneshot::Receiver>, or pend forever if None.
/// Returns None if the sender was dropped (e.g. background task panicked).
async fn recv_or_pend<T>(rx: &mut Option<tokio::sync::oneshot::Receiver<T>>) -> Option<T> {
    match rx {
        Some(r) => {
            let val = r.await.ok();
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
    const TICK_MS: u64 = 250;
    let mut tick = tokio::time::interval(Duration::from_millis(TICK_MS));
    // How often to refresh the progress animation of a running task. Progress is
    // purely cosmetic, so we redraw it about once per second instead of on every
    // tick to avoid spending CPU on terminal redraws during long copies/moves.
    // Task completion, conflicts and status messages are tracked by `snapshot`
    // and still redraw immediately.
    const TASK_REDRAW_EVERY_N_TICKS: u32 = if 1000 / TICK_MS > 0 {
        (1000 / TICK_MS) as u32
    } else {
        1
    };
    // Minimum wall-clock gap between two consecutive redraws driven by *background*
    // work (task progress, streaming dir/find loads, etc.). This keeps the terminal
    // from being redrawn dozens of times per second when background channels fire
    // rapidly. Direct keyboard / resize input bypasses this cap (`draw_immediately`)
    // so navigation always feels instant. A pending background redraw that is held
    // back is flushed by the next 250ms `tick`.
    const MIN_REDRAW_INTERVAL: Duration = Duration::from_millis(TICK_MS);
    let mut last_draw: Option<Instant> = None;
    // First frame must render unconditionally.
    let mut draw_immediately = true;

    loop {
        if app.needs_redraw {
            let due =
                draw_immediately || last_draw.is_none_or(|t| t.elapsed() >= MIN_REDRAW_INTERVAL);
            if due {
                terminal.draw(|f| ui::render(f, app))?;
                app.needs_redraw = false;
                last_draw = Some(Instant::now());
            }
            // Otherwise keep `needs_redraw` set; the next tick flushes it within 250ms.
        }
        draw_immediately = false;

        tokio::select! {
            maybe_event = reader.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        app.handle_key(key);
                        app.needs_redraw = true;
                        draw_immediately = true;
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        app.needs_redraw = true;
                        draw_immediately = true;
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        app.handle_mouse(mouse);
                        app.needs_redraw = true;
                        draw_immediately = true;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => {}
                    None => break,
                }
            }
            _ = tick.tick() => {
                app.tick_count = app.tick_count.wrapping_add(1);
                let before = snapshot(app);
                app.poll_tasks();
                app.poll_conflicts();
                app.poll_du();
                app.poll_find();
                app.poll_info_du();
                app.poll_git();
                if before != snapshot(app) {
                    app.needs_redraw = true;
                }
                // Pending key may need a redraw for which-key popup after delay
                if app.pending_key.is_some() {
                    app.needs_redraw = true;
                }
                // Active tasks (copy/move/delete) animate their progress, but only
                // about once per second — see TASK_REDRAW_EVERY_N_TICKS.
                if app.task_manager.active_count() > 0
                    && app.tick_count.is_multiple_of(TASK_REDRAW_EVERY_N_TICKS)
                {
                    app.needs_redraw = true;
                }
                // Animate the background-work spinner (e.g. dir-size calculation).
                if app.background_progress.is_some() {
                    app.needs_redraw = true;
                }
            }
            Some(msg) = app.dir_load_rx.recv() => {
                // A `Finished` message is the authoritative, final result of a
                // directory load the user just triggered (e.g. by entering a dir).
                // It must render right away. Streaming `Batch` messages are cosmetic
                // progressive hints for very large dirs and stay throttled to avoid
                // redraw storms.
                if matches!(msg, app::DirLoadMsg::Finished { .. }) {
                    draw_immediately = true;
                }
                app.handle_dir_load_msg(msg);
                app.needs_redraw = true;
            }
            result = recv_or_pend(&mut app.preview_load_rx) => {
                if let Some(r) = result { app.apply_preview_load(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.viewer_load_rx) => {
                if let Some(r) = result { app.apply_viewer_load(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.viewer_chunk_rx) => {
                if let Some(r) = result { app.apply_viewer_chunk(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.viewer_hl_rx) => {
                if let Some(r) = result { app.apply_viewer_highlight(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.tree_load_rx) => {
                if let Some(r) = result { app.apply_tree_data(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.info_load_rx) => {
                if let Some(r) = result { app.apply_info_load(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.chown_load_rx) => {
                if let Some(r) = result { app.apply_chown_load(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.nav_check_rx) => {
                if let Some(r) = result { app.apply_nav_check(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.file_op_rx) => {
                if let Some(r) = result { app.apply_file_op(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.theme_load_rx) => {
                if let Some(r) = result { app.apply_theme_preview(r); app.needs_redraw = true; draw_immediately = true; }
            }
            result = recv_or_pend(&mut app.archive_load_rx) => {
                if let Some(r) = result { app.handle_archive_load(r); app.needs_redraw = true; draw_immediately = true; }
            }
        }

        if let Some(path) = app.open_editor.take() {
            open_in_editor(terminal, app, &path)?;
            app.needs_redraw = true;
        }

        if app.should_quit {
            app.save_session();
            return Ok(());
        }
    }
    Ok(())
}
