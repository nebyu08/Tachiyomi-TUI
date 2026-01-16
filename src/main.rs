mod backend;
mod ui;

use backend::mangadex::{
    fetch_cover_image, fetch_page_image, get_chapter_pages, get_manga_chapters,
    get_popular_now, get_recently_updated, search_manga, Manga,
};
use image::DynamicImage;
use ui::ui::{App, Focus, Tab, View, ui};

use crossterm::{
    event::{Event, EventStream, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io};
use tokio::sync::mpsc;

enum BackgroundTask {
    CoverLoaded { manga_id: String, image: DynamicImage },
    ChaptersLoaded { chapters: Vec<backend::mangadex::Chapter> },
    PageUrlsLoaded { urls: Vec<String> },
    PageImageLoaded { image: DynamicImage },
    SearchResults { results: Vec<Manga> },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Create channel for background tasks
    let (task_tx, mut task_rx) = mpsc::unbounded_channel::<BackgroundTask>();

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
    spawn_cover_loaders(&app.recently_updated, 0, 6, task_tx.clone());
    spawn_cover_loaders(&app.popular_now, 0, 6, task_tx.clone());

    // Data loaded, switch to ready state
    app.set_ready();

    let res = run_app(&mut terminal, &mut app, &mut task_rx, task_tx).await;

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
    tx: mpsc::UnboundedSender<BackgroundTask>,
) {
    for manga in mangas.iter().skip(start).take(count) {
        let manga_id = manga.id.clone();
        let cover_url = manga.cover_url.clone();
        let tx = tx.clone();

        tokio::spawn(async move {
            if let Some(image) = fetch_cover_image(&cover_url).await {
                let _ = tx.send(BackgroundTask::CoverLoaded { manga_id, image });
            }
        });
    }
}

fn spawn_chapters_loader(manga_id: String, tx: mpsc::UnboundedSender<BackgroundTask>) {
    tokio::spawn(async move {
        if let Ok(chapters) = get_manga_chapters(&manga_id).await {
            let _ = tx.send(BackgroundTask::ChaptersLoaded { chapters });
        }
    });
}

fn spawn_page_urls_loader(chapter_id: String, tx: mpsc::UnboundedSender<BackgroundTask>) {
    tokio::spawn(async move {
        if let Some(urls) = get_chapter_pages(&chapter_id).await {
            let _ = tx.send(BackgroundTask::PageUrlsLoaded { urls });
        }
    });
}

fn spawn_page_image_loader(page_url: String, tx: mpsc::UnboundedSender<BackgroundTask>) {
    tokio::spawn(async move {
        if let Some(image) = fetch_page_image(&page_url).await {
            let _ = tx.send(BackgroundTask::PageImageLoaded { image });
        }
    });
}

fn spawn_search(query: String, tx: mpsc::UnboundedSender<BackgroundTask>) {
    tokio::spawn(async move {
        if let Ok(results) = search_manga(&query).await {
            let _ = tx.send(BackgroundTask::SearchResults { results });
        } else {
            let _ = tx.send(BackgroundTask::SearchResults { results: Vec::new() });
        }
    });
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    task_rx: &mut mpsc::UnboundedReceiver<BackgroundTask>,
    task_tx: mpsc::UnboundedSender<BackgroundTask>,
) -> io::Result<()> {
    let mut event_stream = EventStream::new();
    let mut pending_covers: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Track which manga IDs are already loading
    for manga in app.recently_updated.iter().take(6) {
        pending_covers.insert(manga.id.clone());
    }
    for manga in app.popular_now.iter().take(6) {
        pending_covers.insert(manga.id.clone());
    }

    const DEBOUNCE_MS: u64 = 300;

    loop {
        terminal.draw(|f| ui(f, app))?;

        // Check if we need to trigger a debounced search
        if let Some(debounce_time) = app.search_debounce {
            if debounce_time.elapsed().as_millis() >= DEBOUNCE_MS as u128 {
                app.search_debounce = None;
                if !app.search_query.is_empty() 
                    && !app.searching 
                    && app.search_query != app.last_search_query 
                {
                    app.searching = true;
                    app.last_search_query = app.search_query.clone();
                    spawn_search(app.search_query.clone(), task_tx.clone());
                }
            }
        }

        tokio::select! {
            // Timeout to check debounce timer
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {}

            // Handle keyboard events
            Some(Ok(event)) = event_stream.next() => {
                if let Event::Key(key) = event {
                    match app.view {
                        View::Home => handle_home_input(app, key.code, &mut pending_covers, &task_tx),
                        View::MangaDetail => handle_detail_input(app, key.code, &task_tx),
                        View::Reader => handle_reader_input(app, key.code, &task_tx),
                    }
                    
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                }
            }

            // Handle background task results
            Some(task) = task_rx.recv() => {
                match task {
                    BackgroundTask::CoverLoaded { manga_id, image } => {
                        app.add_cover_image(&manga_id, image);
                        pending_covers.remove(&manga_id);
                    }
                    BackgroundTask::ChaptersLoaded { chapters } => {
                        app.chapters = chapters;
                    }
                    BackgroundTask::PageUrlsLoaded { urls } => {
                        app.reader.page_urls = urls;
                        // Load first page
                        if let Some(url) = app.reader.page_urls.first() {
                            spawn_page_image_loader(url.clone(), task_tx.clone());
                        }
                    }
                    BackgroundTask::PageImageLoaded { image } => {
                        app.set_page_image(image);
                    }
                    BackgroundTask::SearchResults { results } => {
                        app.search_results = results;
                        app.searching = false;
                        app.search_offset = 0;
                        // Load covers for search results
                        spawn_cover_loaders(&app.search_results, 0, 6, task_tx.clone());
                        for manga in app.search_results.iter().take(6) {
                            pending_covers.insert(manga.id.clone());
                        }
                    }
                }
            }
        }
    }
}

fn handle_home_input(
    app: &mut App,
    key: KeyCode,
    pending_covers: &mut std::collections::HashSet<String>,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
) {
    match app.tab {
        Tab::Home => handle_home_tab_input(app, key, pending_covers, task_tx),
        Tab::Bookmarks => handle_bookmarks_tab_input(app, key, pending_covers, task_tx),
        Tab::Search => handle_search_tab_input(app, key, pending_covers, task_tx),
    }
}

fn handle_home_tab_input(
    app: &mut App,
    key: KeyCode,
    pending_covers: &mut std::collections::HashSet<String>,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
) {
    match key {
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
        KeyCode::Left => match app.focus {
            Focus::Header => {
                app.tab = Tab::Search;
            }
            Focus::Recent => {
                app.recent_offset = app.recent_offset.saturating_sub(1);
            }
            Focus::Popular => {
                app.popular_offset = app.popular_offset.saturating_sub(1);
            }
        },
        KeyCode::Right => match app.focus {
            Focus::Header => {
                app.tab = Tab::Bookmarks;
            }
            Focus::Recent => {
                app.recent_offset += 1;
                preload_covers(
                    &app.recently_updated,
                    app.recent_offset,
                    pending_covers,
                    &app.image_states,
                    task_tx.clone(),
                );
            }
            Focus::Popular => {
                app.popular_offset += 1;
                preload_covers(
                    &app.popular_now,
                    app.popular_offset,
                    pending_covers,
                    &app.image_states,
                    task_tx.clone(),
                );
            }
        },
        KeyCode::Enter => {
            let manga = match app.focus {
                Focus::Recent => app.recently_updated.get(app.recent_offset).cloned(),
                Focus::Popular => app.popular_now.get(app.popular_offset).cloned(),
                Focus::Header => None,
            };
            
            if let Some(manga) = manga {
                let manga_id = manga.id.clone();
                app.open_manga(manga);
                spawn_chapters_loader(manga_id, task_tx.clone());
            }
        }
        _ => {}
    }
}

fn handle_bookmarks_tab_input(
    app: &mut App,
    key: KeyCode,
    pending_covers: &mut std::collections::HashSet<String>,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
) {
    let bookmarked = app.bookmarks.get_bookmarked_manga();
    
    match key {
        KeyCode::Left => {
            if app.focus == Focus::Header {
                app.tab = Tab::Home;
            } else {
                app.bookmark_offset = app.bookmark_offset.saturating_sub(1);
            }
        }
        KeyCode::Right => {
            if app.focus == Focus::Header {
                app.tab = Tab::Search;
            } else if !bookmarked.is_empty() {
                let max_offset = bookmarked.len().saturating_sub(1);
                if app.bookmark_offset < max_offset {
                    app.bookmark_offset += 1;
                    preload_covers(
                        &bookmarked,
                        app.bookmark_offset,
                        pending_covers,
                        &app.image_states,
                        task_tx.clone(),
                    );
                }
            }
        }
        KeyCode::Tab | KeyCode::Down => {
            app.focus = Focus::Recent;
        }
        KeyCode::Up => {
            app.focus = Focus::Header;
        }
        KeyCode::Enter => {
            if app.focus != Focus::Header {
                if let Some(manga) = bookmarked.get(app.bookmark_offset).cloned() {
                    let manga_id = manga.id.clone();
                    app.open_manga(manga);
                    spawn_chapters_loader(manga_id, task_tx.clone());
                }
            }
        }
        _ => {}
    }
}

fn handle_search_tab_input(
    app: &mut App,
    key: KeyCode,
    pending_covers: &mut std::collections::HashSet<String>,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
) {
    match key {
        KeyCode::Char(c) => {
            app.search_query.push(c);
            app.search_debounce = Some(std::time::Instant::now());
        }
        KeyCode::Backspace => {
            app.search_query.pop();
            if app.search_query.is_empty() {
                app.search_results.clear();
                app.last_search_query.clear();
                app.search_debounce = None;
            } else {
                app.search_debounce = Some(std::time::Instant::now());
            }
        }
        KeyCode::Enter => {
            if app.focus == Focus::Header {
                // Immediate search on Enter
                if !app.search_query.is_empty() && !app.searching {
                    app.searching = true;
                    app.last_search_query = app.search_query.clone();
                    app.search_debounce = None;
                    spawn_search(app.search_query.clone(), task_tx.clone());
                }
            } else {
                // Open manga when focused on results
                if let Some(manga) = app.search_results.get(app.search_offset).cloned() {
                    let manga_id = manga.id.clone();
                    app.open_manga(manga);
                    spawn_chapters_loader(manga_id, task_tx.clone());
                }
            }
        }
        KeyCode::Left => {
            if app.focus == Focus::Header {
                app.tab = Tab::Bookmarks;
            } else {
                app.search_offset = app.search_offset.saturating_sub(1);
            }
        }
        KeyCode::Right => {
            if app.focus == Focus::Header {
                app.tab = Tab::Home;
            } else if !app.search_results.is_empty() {
                let max_offset = app.search_results.len().saturating_sub(1);
                if app.search_offset < max_offset {
                    app.search_offset += 1;
                    preload_covers(
                        &app.search_results,
                        app.search_offset,
                        pending_covers,
                        &app.image_states,
                        task_tx.clone(),
                    );
                }
            }
        }
        KeyCode::Tab | KeyCode::Down => {
            app.focus = Focus::Recent;
        }
        KeyCode::Up => {
            app.focus = Focus::Header;
        }
        KeyCode::Esc => {
            if app.focus != Focus::Header {
                app.focus = Focus::Header;
            } else {
                app.search_query.clear();
                app.search_results.clear();
            }
        }
        _ => {}
    }
}

fn handle_detail_input(
    app: &mut App,
    key: KeyCode,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
) {
    match key {
        KeyCode::Esc => {
            app.go_back();
        }
        KeyCode::Up => {
            let selected = app.chapter_list_state.selected().unwrap_or(0);
            if selected > 0 {
                app.chapter_list_state.select(Some(selected - 1));
            }
        }
        KeyCode::Down => {
            let selected = app.chapter_list_state.selected().unwrap_or(0);
            if selected + 1 < app.chapters.len() {
                app.chapter_list_state.select(Some(selected + 1));
            }
        }
        KeyCode::Enter => {
            if let Some(selected) = app.chapter_list_state.selected() {
                if let Some(chapter) = app.chapters.get(selected) {
                    let chapter_id = chapter.id.clone();
                    app.open_reader(selected);
                    spawn_page_urls_loader(chapter_id, task_tx.clone());
                }
            }
        }
        KeyCode::Char('b') => {
            app.toggle_bookmark();
        }
        _ => {}
    }
}

fn handle_reader_input(
    app: &mut App,
    key: KeyCode,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
) {
    match key {
        KeyCode::Esc => {
            app.go_back();
        }
        KeyCode::Left => {
            if app.prev_page() {
                if let Some(url) = app.reader.page_urls.get(app.reader.current_page) {
                    spawn_page_image_loader(url.clone(), task_tx.clone());
                }
            }
        }
        KeyCode::Right => {
            if app.next_page() {
                if let Some(url) = app.reader.page_urls.get(app.reader.current_page) {
                    spawn_page_image_loader(url.clone(), task_tx.clone());
                }
            }
        }
        KeyCode::Char('n') => {
            if app.next_chapter() {
                if let Some(chapter) = app.reader.chapters.get(app.reader.current_chapter_idx) {
                    spawn_page_urls_loader(chapter.id.clone(), task_tx.clone());
                }
            }
        }
        KeyCode::Char('p') => {
            if app.prev_chapter() {
                if let Some(chapter) = app.reader.chapters.get(app.reader.current_chapter_idx) {
                    spawn_page_urls_loader(chapter.id.clone(), task_tx.clone());
                }
            }
        }
        _ => {}
    }
}

fn preload_covers(
    mangas: &[Manga],
    offset: usize,
    pending: &mut std::collections::HashSet<String>,
    loaded: &std::collections::HashMap<String, ratatui_image::protocol::StatefulProtocol>,
    tx: mpsc::UnboundedSender<BackgroundTask>,
) {
    for manga in mangas.iter().skip(offset).take(8) {
        if !loaded.contains_key(&manga.id) && !pending.contains(&manga.id) {
            pending.insert(manga.id.clone());
            let manga_id = manga.id.clone();
            let cover_url = manga.cover_url.clone();
            let tx = tx.clone();

            tokio::spawn(async move {
                if let Some(image) = fetch_cover_image(&cover_url).await {
                    let _ = tx.send(BackgroundTask::CoverLoaded { manga_id, image });
                }
            });
        }
    }
}
