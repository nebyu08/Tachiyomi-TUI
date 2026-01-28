mod backend;
mod ui;

use backend::cache::PageCache;
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
    ChapterThumbnailLoaded { chapter_id: String, image: DynamicImage },
    PageUrlsLoaded { urls: Vec<String> },
    PageUrlsLoadFailed,
    PageImageLoaded { image: DynamicImage },
    PageImageLoadFailed,
    PagePreloaded { page_url: String },
    SearchResults { results: Vec<Manga> },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    log::debug!("Starting manga reader...");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let cache = PageCache::new();

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

    let res = run_app(&mut terminal, &mut app, &mut task_rx, task_tx, cache).await;

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

fn spawn_chapter_thumbnail_loader(
    chapter_id: String,
    tx: mpsc::UnboundedSender<BackgroundTask>,
    cache: PageCache,
) {
    tokio::spawn(async move {
        if let Some(image) = load_chapter_thumbnail(&chapter_id, &cache).await {
            let _ = tx.send(BackgroundTask::ChapterThumbnailLoaded { chapter_id, image });
        }
    });
}

fn spawn_chapter_thumbnails_preloader(
    chapters: Vec<backend::mangadex::Chapter>,
    tx: mpsc::UnboundedSender<BackgroundTask>,
    cache: PageCache,
) {
    tokio::spawn(async move {
        for chapter in chapters.iter() {
            if chapter.external_url.is_some() {
                continue;
            }
            
            // Small delay between requests to avoid rate limiting
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            
            if let Some(image) = load_chapter_thumbnail(&chapter.id, &cache).await {
                let _ = tx.send(BackgroundTask::ChapterThumbnailLoaded { 
                    chapter_id: chapter.id.clone(), 
                    image 
                });
            }
        }
    });
}

async fn load_chapter_thumbnail(chapter_id: &str, cache: &PageCache) -> Option<DynamicImage> {
    // Check if we have cached URLs for this chapter
    if let Some(urls) = cache.get_chapter_urls(chapter_id).await {
        if let Some(first_url) = urls.first() {
            return fetch_first_page_thumbnail(first_url, cache).await;
        }
    }

    // Fetch URLs from API
    if let Some(urls) = get_chapter_pages(chapter_id).await {
        if !urls.is_empty() {
            cache.insert_chapter_urls(chapter_id.to_string(), urls.clone()).await;
            if let Some(first_url) = urls.first() {
                return fetch_first_page_thumbnail(first_url, cache).await;
            }
        }
    }
    
    None
}

async fn fetch_first_page_thumbnail(page_url: &str, cache: &PageCache) -> Option<DynamicImage> {
    // Check disk/memory cache first
    if let Some(image) = cache.get_page(page_url).await {
        return Some(image);
    }
    
    // Fetch from network and cache
    if let Some(image) = fetch_page_image(page_url).await {
        cache.insert_page(page_url.to_string(), image.clone()).await;
        return Some(image);
    }
    
    None
}

fn spawn_page_urls_loader(chapter_id: String, tx: mpsc::UnboundedSender<BackgroundTask>, cache: PageCache) {
    log::debug!("Loading page URLs for chapter: {}", chapter_id);
    tokio::spawn(async move {
        if let Some(cached_urls) = cache.get_chapter_urls(&chapter_id).await {
            log::debug!("Found cached URLs for chapter {}: {} pages", chapter_id, cached_urls.len());
            let _ = tx.send(BackgroundTask::PageUrlsLoaded { urls: cached_urls });
            return;
        }

        log::debug!("Fetching page URLs from API for chapter: {}", chapter_id);
        match get_chapter_pages(&chapter_id).await {
            Some(urls) => {
                if !urls.is_empty() {
                    log::debug!("Loaded {} page URLs for chapter {}", urls.len(), chapter_id);
                    cache.insert_chapter_urls(chapter_id, urls.clone()).await;
                    let _ = tx.send(BackgroundTask::PageUrlsLoaded { urls });
                } else {
                    log::error!("Chapter {} has empty page URLs", chapter_id);
                    let _ = tx.send(BackgroundTask::PageUrlsLoadFailed);
                }
            }
            None => {
                log::error!("Failed to fetch page URLs for chapter {}", chapter_id);
                let _ = tx.send(BackgroundTask::PageUrlsLoadFailed);
            }
        }
    });
}

fn spawn_page_image_loader(page_url: String, tx: mpsc::UnboundedSender<BackgroundTask>, cache: PageCache) {
    log::debug!("Loading page image: {}", page_url);
    tokio::spawn(async move {
        if let Some(cached_image) = cache.get_page(&page_url).await {
            log::debug!("Found cached image for: {}", page_url);
            let _ = tx.send(BackgroundTask::PageImageLoaded { image: cached_image });
            return;
        }

        const MAX_RETRIES: u32 = 3;
        for attempt in 0..MAX_RETRIES {
            log::debug!("Attempt {} to fetch image: {}", attempt + 1, page_url);
            if let Some(image) = fetch_page_image(&page_url).await {
                log::debug!("Successfully loaded image (attempt {})", attempt + 1);
                cache.insert_page(page_url, image.clone()).await;
                let _ = tx.send(BackgroundTask::PageImageLoaded { image });
                return;
            }
            if attempt < MAX_RETRIES - 1 {
                let delay = 500 * (attempt as u64 + 1);
                log::warn!("Image fetch failed, retrying in {}ms", delay);
                tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
            }
        }
        log::error!("Failed to load image after {} retries: {}", MAX_RETRIES, page_url);
        let _ = tx.send(BackgroundTask::PageImageLoadFailed);
    });
}

fn spawn_page_preloader(page_url: String, tx: mpsc::UnboundedSender<BackgroundTask>, cache: PageCache) {
    tokio::spawn(async move {
        if cache.has_page(&page_url).await {
            let _ = tx.send(BackgroundTask::PagePreloaded { page_url });
            return;
        }

        if let Some(image) = fetch_page_image(&page_url).await {
            cache.insert_page(page_url.clone(), image).await;
            let _ = tx.send(BackgroundTask::PagePreloaded { page_url });
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
    cache: PageCache,
) -> io::Result<()> {
    let mut event_stream = EventStream::new();
    let mut pending_covers: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut preloading_pages: std::collections::HashSet<String> = std::collections::HashSet::new();

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
                        View::Home => handle_home_input(app, key.code, &mut pending_covers, &task_tx, &cache),
                        View::MangaDetail => handle_detail_input(app, key.code, &task_tx, &cache),
                        View::Reader => handle_reader_input(app, key.code, &task_tx, &cache, &mut preloading_pages),
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
                        app.chapters = chapters.clone();
                        // Preload all chapter thumbnails in background
                        spawn_chapter_thumbnails_preloader(
                            chapters,
                            task_tx.clone(),
                            cache.clone(),
                        );
                    }
                    BackgroundTask::ChapterThumbnailLoaded { chapter_id, image } => {
                        app.add_chapter_thumbnail(&chapter_id, image);
                    }
                    BackgroundTask::PageUrlsLoaded { urls } => {
                        app.reader.page_urls = urls;
                        app.reader.error = None;
                        // Load first page
                        if let Some(url) = app.reader.page_urls.first() {
                            spawn_page_image_loader(url.clone(), task_tx.clone(), cache.clone());
                        }
                        // Preload next few pages in background
                        preload_upcoming_pages(
                            &app.reader.page_urls,
                            0,
                            &mut preloading_pages,
                            &task_tx,
                            &cache,
                        );
                    }
                    BackgroundTask::PageUrlsLoadFailed => {
                        app.set_page_load_error("Failed to load chapter pages. Press 'r' to retry.".to_string());
                    }
                    BackgroundTask::PageImageLoaded { image } => {
                        app.set_page_image(image);
                        // Preload upcoming pages when current page loads
                        preload_upcoming_pages(
                            &app.reader.page_urls,
                            app.reader.current_page,
                            &mut preloading_pages,
                            &task_tx,
                            &cache,
                        );
                    }
                    BackgroundTask::PageImageLoadFailed => {
                        app.set_page_load_error("Failed to load page image. Press 'r' to retry.".to_string());
                    }
                    BackgroundTask::PagePreloaded { page_url } => {
                        preloading_pages.remove(&page_url);
                        // Continue preloading from this page's position
                        if let Some(idx) = app.reader.page_urls.iter().position(|u| u == &page_url) {
                            preload_upcoming_pages(
                                &app.reader.page_urls,
                                idx,
                                &mut preloading_pages,
                                &task_tx,
                                &cache,
                            );
                        }
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
    cache: &PageCache,
) {
    match app.tab {
        Tab::Home => handle_home_tab_input(app, key, pending_covers, task_tx, cache),
        Tab::Bookmarks => handle_bookmarks_tab_input(app, key, pending_covers, task_tx, cache),
        Tab::Search => handle_search_tab_input(app, key, pending_covers, task_tx, cache),
    }
}

fn handle_home_tab_input(
    app: &mut App,
    key: KeyCode,
    pending_covers: &mut std::collections::HashSet<String>,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
    _cache: &PageCache,
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
    _cache: &PageCache,
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
    _cache: &PageCache,
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
    cache: &PageCache,
) {
    let cols = app.chapter_grid_cols.max(1);
    
    match key {
        KeyCode::Esc => {
            app.go_back();
        }
        KeyCode::Left => {
            if app.chapter_selected > 0 {
                app.chapter_selected -= 1;
                preload_chapter_thumbnails(app, app.chapter_selected, task_tx, cache);
            }
        }
        KeyCode::Right => {
            if app.chapter_selected + 1 < app.chapters.len() {
                app.chapter_selected += 1;
                preload_chapter_thumbnails(app, app.chapter_selected, task_tx, cache);
            }
        }
        KeyCode::Up => {
            if app.chapter_selected >= cols {
                app.chapter_selected -= cols;
                preload_chapter_thumbnails(app, app.chapter_selected, task_tx, cache);
            }
        }
        KeyCode::Down => {
            let new_idx = app.chapter_selected + cols;
            if new_idx < app.chapters.len() {
                app.chapter_selected = new_idx;
                preload_chapter_thumbnails(app, app.chapter_selected, task_tx, cache);
            }
        }
        KeyCode::Enter => {
            if let Some(chapter) = app.chapters.get(app.chapter_selected) {
                if let Some(external_url) = &chapter.external_url {
                    log::debug!("Chapter is external and cannot be read in-app: {}", external_url);
                    webbrowser::open(external_url).ok();
                } else {
                    let chapter_id = chapter.id.clone();
                    app.open_reader(app.chapter_selected);
                    spawn_page_urls_loader(chapter_id, task_tx.clone(), cache.clone());
                }
            }
        }
        KeyCode::Char('b') => {
            app.toggle_bookmark();
        }
        _ => {}
    }
}

fn preload_chapter_thumbnails(
    app: &App,
    current_idx: usize,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
    cache: &PageCache,
) {
    // Only load thumbnail for the currently selected chapter to avoid rate limiting
    if let Some(chapter) = app.chapters.get(current_idx) {
        if chapter.external_url.is_none() && !app.chapter_thumbnails.contains_key(&chapter.id) {
            spawn_chapter_thumbnail_loader(
                chapter.id.clone(),
                task_tx.clone(),
                cache.clone(),
            );
        }
    }
}

fn handle_reader_input(
    app: &mut App,
    key: KeyCode,
    task_tx: &mpsc::UnboundedSender<BackgroundTask>,
    cache: &PageCache,
    preloading_pages: &mut std::collections::HashSet<String>,
) {
    match key {
        KeyCode::Esc => {
            app.go_back();
        }
        KeyCode::Left => {
            if app.prev_page() {
                if let Some(url) = app.reader.page_urls.get(app.reader.current_page) {
                    spawn_page_image_loader(url.clone(), task_tx.clone(), cache.clone());
                }
            }
        }
        KeyCode::Right => {
            if app.next_page() {
                if let Some(url) = app.reader.page_urls.get(app.reader.current_page) {
                    spawn_page_image_loader(url.clone(), task_tx.clone(), cache.clone());
                }
                preload_upcoming_pages(
                    &app.reader.page_urls,
                    app.reader.current_page,
                    preloading_pages,
                    task_tx,
                    cache,
                );
            }
        }
        KeyCode::Char('n') => {
            if app.next_chapter() {
                if let Some(chapter) = app.reader.chapters.get(app.reader.current_chapter_idx) {
                    spawn_page_urls_loader(chapter.id.clone(), task_tx.clone(), cache.clone());
                }
            }
        }
        KeyCode::Char('p') => {
            if app.prev_chapter() {
                if let Some(chapter) = app.reader.chapters.get(app.reader.current_chapter_idx) {
                    spawn_page_urls_loader(chapter.id.clone(), task_tx.clone(), cache.clone());
                }
            }
        }
        KeyCode::Char('r') => {
            if app.reader.error.is_some() {
                app.reader.loading = true;
                app.reader.error = None;
                if app.reader.page_urls.is_empty() {
                    if let Some(chapter) = app.reader.chapters.get(app.reader.current_chapter_idx) {
                        spawn_page_urls_loader(chapter.id.clone(), task_tx.clone(), cache.clone());
                    }
                } else if let Some(url) = app.reader.page_urls.get(app.reader.current_page) {
                    spawn_page_image_loader(url.clone(), task_tx.clone(), cache.clone());
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

fn preload_upcoming_pages(
    page_urls: &[String],
    current_page: usize,
    preloading: &mut std::collections::HashSet<String>,
    tx: &mpsc::UnboundedSender<BackgroundTask>,
    cache: &PageCache,
) {
    const PRELOAD_AHEAD: usize = 3;

    for url in page_urls.iter().skip(current_page + 1).take(PRELOAD_AHEAD) {
        if !preloading.contains(url) {
            preloading.insert(url.clone());
            spawn_page_preloader(url.clone(), tx.clone(), cache.clone());
        }
    }
}
