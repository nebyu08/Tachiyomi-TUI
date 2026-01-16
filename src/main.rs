mod backend;
mod ui;

use backend::mangadex::{fetch_cover_image, get_popular_now, get_recently_updated, Manga};
use image::DynamicImage;
use ui::ui::{App, Focus, Tab, ui};

use crossterm::{
    event::{Event, EventStream, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io};
use tokio::sync::mpsc;

struct ImageLoadResult {
    manga_id: String,
    image: DynamicImage,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Create channel for background image loading
    let (image_tx, mut image_rx) = mpsc::unbounded_channel::<ImageLoadResult>();

    // Show loading screen
    app.set_loading("Connecting to MangaDex...");
    terminal.draw(|f| ui(f, &mut app))?;

    // Fetch manga data
    app.set_loading("Fetching recently updated manga...");
    terminal.draw(|f| ui(f, &mut app))?;

    let recent_manga = get_recently_updated().await.unwrap_or_default();

    app.set_loading("Fetching popular manga...");
    terminal.draw(|f| ui(f, &mut app))?;

    let popular_manga = get_popular_now().await.unwrap_or_default();

    // Store manga data
    app.recently_updated = recent_manga;
    app.popular_now = popular_manga;

    // Spawn background tasks to load initial covers
    spawn_cover_loaders(&app.recently_updated, 0, 6, image_tx.clone());
    spawn_cover_loaders(&app.popular_now, 0, 6, image_tx.clone());

    // Data loaded, switch to ready state
    app.set_ready();

    let res = run_app(&mut terminal, &mut app, &mut image_rx, image_tx).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("{err}");
    }
    Ok(())
}

fn spawn_cover_loaders(
    mangas: &[Manga],
    start: usize,
    count: usize,
    tx: mpsc::UnboundedSender<ImageLoadResult>,
) {
    for manga in mangas.iter().skip(start).take(count) {
        let manga_id = manga.id.clone();
        let cover_url = manga.cover_url.clone();
        let tx = tx.clone();

        tokio::spawn(async move {
            if let Some(image) = fetch_cover_image(&cover_url).await {
                let _ = tx.send(ImageLoadResult { manga_id, image });
            }
        });
    }
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    image_rx: &mut mpsc::UnboundedReceiver<ImageLoadResult>,
    image_tx: mpsc::UnboundedSender<ImageLoadResult>,
) -> io::Result<()> {
    let mut event_stream = EventStream::new();
    let mut pending_recent: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut pending_popular: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Track which manga IDs are already loading
    for manga in app.recently_updated.iter().take(6) {
        pending_recent.insert(manga.id.clone());
    }
    for manga in app.popular_now.iter().take(6) {
        pending_popular.insert(manga.id.clone());
    }

    loop {
        terminal.draw(|f| ui(f, app))?;

        tokio::select! {
            // Handle keyboard events
            Some(Ok(event)) = event_stream.next() => {
                if let Event::Key(key) = event {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(()),

                        KeyCode::Tab | KeyCode::Down => {
                            app.focus = match app.focus {
                                Focus::Header => Focus::Recent,
                                Focus::Recent => Focus::Popular,
                                Focus::Popular => Focus::Header,
                            }
                        }
                        KeyCode::Up => {
                            app.focus = match app.focus {
                                Focus::Header => Focus::Popular,
                                Focus::Recent => Focus::Header,
                                Focus::Popular => Focus::Recent,
                            }
                        }

                        KeyCode::Left => {
                            match app.focus {
                                Focus::Header => {
                                    app.tab = match app.tab {
                                        Tab::Home => Tab::Search,
                                        Tab::Bookmarks => Tab::Home,
                                        Tab::Search => Tab::Bookmarks,
                                    }
                                }
                                Focus::Recent => {
                                    app.recent_offset = app.recent_offset.saturating_sub(1);
                                }
                                Focus::Popular => {
                                    app.popular_offset = app.popular_offset.saturating_sub(1);
                                }
                            }
                        }
                        KeyCode::Right => {
                            match app.focus {
                                Focus::Header => {
                                    app.tab = match app.tab {
                                        Tab::Home => Tab::Bookmarks,
                                        Tab::Bookmarks => Tab::Search,
                                        Tab::Search => Tab::Home,
                                    }
                                }
                                Focus::Recent => {
                                    app.recent_offset += 1;
                                    // Preload next covers
                                    preload_covers(
                                        &app.recently_updated,
                                        app.recent_offset,
                                        &mut pending_recent,
                                        &app.image_states,
                                        image_tx.clone(),
                                    );
                                }
                                Focus::Popular => {
                                    app.popular_offset += 1;
                                    // Preload next covers
                                    preload_covers(
                                        &app.popular_now,
                                        app.popular_offset,
                                        &mut pending_popular,
                                        &app.image_states,
                                        image_tx.clone(),
                                    );
                                }
                            }
                        }

                        KeyCode::Backspace => {
                            if app.tab == Tab::Search {
                                app.search_query.pop();
                            }
                        }
                        KeyCode::Char(c) => {
                            if app.tab == Tab::Search && app.focus == Focus::Header {
                                app.search_query.push(c);
                            }
                        }

                        _ => {}
                    }
                }
            }

            // Handle loaded images from background tasks
            Some(result) = image_rx.recv() => {
                app.add_cover_image(&result.manga_id, result.image);
                pending_recent.remove(&result.manga_id);
                pending_popular.remove(&result.manga_id);
            }
        }
    }
}

fn preload_covers(
    mangas: &[Manga],
    offset: usize,
    pending: &mut std::collections::HashSet<String>,
    loaded: &std::collections::HashMap<String, ratatui_image::protocol::StatefulProtocol>,
    tx: mpsc::UnboundedSender<ImageLoadResult>,
) {
    // Preload covers for visible range + buffer
    for manga in mangas.iter().skip(offset).take(8) {
        if !loaded.contains_key(&manga.id) && !pending.contains(&manga.id) {
            pending.insert(manga.id.clone());
            let manga_id = manga.id.clone();
            let cover_url = manga.cover_url.clone();
            let tx = tx.clone();

            tokio::spawn(async move {
                if let Some(image) = fetch_cover_image(&cover_url).await {
                    let _ = tx.send(ImageLoadResult { manga_id, image });
                }
            });
        }
    }
}
