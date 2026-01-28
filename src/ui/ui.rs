use image::DynamicImage;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, ListState, Paragraph, Tabs},
    Frame,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, Resize, StatefulImage};
use std::collections::HashMap;

use crate::backend::bookmarks::Bookmarks;
use crate::backend::mangadex::{Chapter, Manga};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Home,
    Bookmarks,
    Search,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Header,
    Recent,
    Popular,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AppState {
    #[default]
    Loading,
    Ready,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Home,
    MangaDetail,
    Reader,
}

#[derive(Default)]
pub struct ReaderState {
    pub manga: Option<Manga>,
    pub chapters: Vec<Chapter>,
    pub current_chapter_idx: usize,
    pub page_urls: Vec<String>,
    pub current_page: usize,
    pub page_image: Option<StatefulProtocol>,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct App {
    pub state: AppState,
    pub view: View,
    pub loading_message: String,
    pub tab: Tab,
    pub focus: Focus,
    pub search_query: String,
    pub search_results: Vec<Manga>,
    pub search_offset: usize,
    pub searching: bool,
    pub last_search_query: String,
    pub search_debounce: Option<std::time::Instant>,
    pub recent_offset: usize,
    pub popular_offset: usize,
    pub bookmark_offset: usize,
    pub recently_updated: Vec<Manga>,
    pub popular_now: Vec<Manga>,
    pub picker: Option<Picker>,
    pub cover_images: HashMap<String, DynamicImage>,
    pub image_states: HashMap<String, StatefulProtocol>,
    pub bookmarks: Bookmarks,
    
    // Manga detail view
    pub selected_manga: Option<Manga>,
    pub chapters: Vec<Chapter>,
    pub chapter_list_state: ListState,
    pub chapter_selected: usize,      // Currently selected chapter index
    pub chapter_scroll_row: usize,    // First visible row
    pub chapter_grid_cols: usize,     // Columns in grid (calculated from width)
    pub chapter_thumbnails: HashMap<String, StatefulProtocol>,
    pub chapter_thumbnail_images: HashMap<String, DynamicImage>,
    
    // Reader view
    pub reader: ReaderState,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let picker = Picker::from_query_stdio().ok();

        Self {
            state: AppState::Loading,
            view: View::Home,
            loading_message: "Initializing...".to_string(),
            tab: Tab::Home,
            focus: Focus::Header,
            search_query: String::new(),
            search_results: Vec::new(),
            search_offset: 0,
            searching: false,
            last_search_query: String::new(),
            search_debounce: None,
            recent_offset: 0,
            popular_offset: 0,
            bookmark_offset: 0,
            recently_updated: Vec::new(),
            popular_now: Vec::new(),
            picker,
            cover_images: HashMap::new(),
            image_states: HashMap::new(),
            bookmarks: Bookmarks::load(),
            selected_manga: None,
            chapters: Vec::new(),
            chapter_list_state: ListState::default(),
            chapter_selected: 0,
            chapter_scroll_row: 0,
            chapter_grid_cols: 1,
            chapter_thumbnails: HashMap::new(),
            chapter_thumbnail_images: HashMap::new(),
            reader: ReaderState::default(),
        }
    }

    pub fn toggle_bookmark(&mut self) {
        if let Some(ref manga) = self.selected_manga {
            self.bookmarks.toggle(manga);
        }
    }

    pub fn is_current_bookmarked(&self) -> bool {
        if let Some(ref manga) = self.selected_manga {
            self.bookmarks.is_bookmarked(&manga.id)
        } else {
            false
        }
    }

    pub fn set_loading(&mut self, message: &str) {
        self.state = AppState::Loading;
        self.loading_message = message.to_string();
    }

    pub fn set_ready(&mut self) {
        self.state = AppState::Ready;
    }

    pub fn add_cover_image(&mut self, manga_id: &str, image: DynamicImage) {
        self.cover_images.insert(manga_id.to_string(), image.clone());

        if let Some(ref picker) = self.picker {
            let protocol = picker.new_resize_protocol(image);
            self.image_states.insert(manga_id.to_string(), protocol);
        }
    }

    pub fn open_manga(&mut self, manga: Manga) {
        self.selected_manga = Some(manga);
        self.view = View::MangaDetail;
        self.chapters.clear();
        self.chapter_list_state.select(Some(0));
        self.chapter_selected = 0;
        self.chapter_scroll_row = 0;
        self.chapter_thumbnails.clear();
        self.chapter_thumbnail_images.clear();
    }

    pub fn add_chapter_thumbnail(&mut self, chapter_id: &str, image: DynamicImage) {
        self.chapter_thumbnail_images.insert(chapter_id.to_string(), image.clone());
        if let Some(ref picker) = self.picker {
            let protocol = picker.new_resize_protocol(image);
            self.chapter_thumbnails.insert(chapter_id.to_string(), protocol);
        }
    }

    pub fn open_reader(&mut self, chapter_idx: usize) {
        self.reader.current_chapter_idx = chapter_idx;
        self.reader.manga = self.selected_manga.clone();
        self.reader.chapters = self.chapters.clone();
        self.reader.current_page = 0;
        self.reader.page_urls.clear();
        self.reader.page_image = None;
        self.reader.loading = true;
        self.view = View::Reader;
    }

    pub fn set_page_image(&mut self, image: DynamicImage) {
        if let Some(ref picker) = self.picker {
            self.reader.page_image = Some(picker.new_resize_protocol(image));
        }
        self.reader.loading = false;
        self.reader.error = None;
    }

    pub fn set_page_load_error(&mut self, error: String) {
        self.reader.loading = false;
        self.reader.error = Some(error);
    }

    pub fn next_page(&mut self) -> bool {
        if self.reader.current_page + 1 < self.reader.page_urls.len() {
            self.reader.current_page += 1;
            self.reader.loading = true;
            self.reader.page_image = None;
            self.reader.error = None;
            true
        } else {
            false
        }
    }

    pub fn prev_page(&mut self) -> bool {
        if self.reader.current_page > 0 {
            self.reader.current_page -= 1;
            self.reader.loading = true;
            self.reader.page_image = None;
            self.reader.error = None;
            true
        } else {
            false
        }
    }

    pub fn next_chapter(&mut self) -> bool {
        if self.reader.current_chapter_idx + 1 < self.reader.chapters.len() {
            self.reader.current_chapter_idx += 1;
            self.reader.current_page = 0;
            self.reader.page_urls.clear();
            self.reader.page_image = None;
            self.reader.loading = true;
            self.reader.error = None;
            true
        } else {
            false
        }
    }

    pub fn prev_chapter(&mut self) -> bool {
        if self.reader.current_chapter_idx > 0 {
            self.reader.current_chapter_idx -= 1;
            self.reader.current_page = 0;
            self.reader.page_urls.clear();
            self.reader.page_image = None;
            self.reader.loading = true;
            self.reader.error = None;
            true
        } else {
            false
        }
    }

    pub fn go_back(&mut self) {
        match self.view {
            View::Reader => self.view = View::MangaDetail,
            View::MangaDetail => {
                self.view = View::Home;
                self.selected_manga = None;
                self.chapters.clear();
            }
            View::Home => {}
        }
    }
}

const CARD_WIDTH: u16 = 35;

pub fn ui(f: &mut Frame, app: &mut App) {
    match app.state {
        AppState::Loading => draw_loading_screen(f, app),
        AppState::Ready => match app.view {
            View::Home => draw_main_ui(f, app),
            View::MangaDetail => draw_manga_detail(f, app),
            View::Reader => draw_reader(f, app),
        },
    }
}

fn draw_loading_screen(f: &mut Frame, app: &App) {
    let area = f.area();

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Manga Reader")
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let center_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Percentage(40),
        ])
        .split(inner);

    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let frame_idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis()
        / 100) as usize
        % spinner_frames.len();

    let spinner = spinner_frames[frame_idx];

    let loading_text = Line::from(vec![
        Span::styled(
            format!(" {} ", spinner),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Loading...",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let loading_paragraph = Paragraph::new(loading_text).alignment(Alignment::Center);
    f.render_widget(loading_paragraph, center_layout[1]);

    let message = Paragraph::new(&*app.loading_message)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(message, center_layout[2]);
}

fn draw_main_ui(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header/tabs
            Constraint::Min(10),   // content (fills remaining space)
            Constraint::Length(3), // footer
        ])
        .split(area);

    draw_header(f, root[0], app);

    match app.tab {
        Tab::Home => draw_home_content(f, root[1], app),
        Tab::Bookmarks => draw_bookmarks_content(f, root[1], app),
        Tab::Search => draw_search_content(f, root[1], app),
    }

    let footer_text = match app.tab {
        Tab::Home => "Tab: section | ←/→: scroll | ↑/↓: focus | Enter: select | q: quit",
        Tab::Bookmarks => "←/→: scroll | Enter: select | q: quit",
        Tab::Search => "Type to search | Enter: search | ←/→: scroll results | q: quit",
    };
    draw_footer(f, root[2], footer_text);
}

fn draw_home_content(f: &mut Frame, area: Rect, app: &mut App) {
    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // recently updated
            Constraint::Percentage(50), // popular now
        ])
        .split(area);

    draw_manga_section(
        f,
        content_layout[0],
        "Recently Updated",
        &app.recently_updated,
        &mut app.recent_offset,
        app.focus == Focus::Recent,
        &mut app.image_states,
    );
    draw_manga_section(
        f,
        content_layout[1],
        "Popular Now",
        &app.popular_now,
        &mut app.popular_offset,
        app.focus == Focus::Popular,
        &mut app.image_states,
    );
}

fn draw_bookmarks_content(f: &mut Frame, area: Rect, app: &mut App) {
    let bookmarked = app.bookmarks.get_bookmarked_manga();
    
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!("Bookmarks ({})", bookmarked.len()))
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if bookmarked.is_empty() {
        let empty_msg = Paragraph::new("No bookmarks yet. Press 'b' on a manga to bookmark it.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty_msg, inner);
        return;
    }

    // Clamp offset
    let max_offset = bookmarked.len().saturating_sub(1);
    if app.bookmark_offset > max_offset {
        app.bookmark_offset = max_offset;
    }

    let available_width = inner.width as usize;
    let cards_visible = (available_width / CARD_WIDTH as usize).max(1);

    let card_constraints: Vec<Constraint> = (0..cards_visible)
        .map(|_| Constraint::Length(CARD_WIDTH))
        .collect();

    let card_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(card_constraints)
        .split(inner);

    for (i, card_area) in card_areas.iter().enumerate() {
        let manga_idx = app.bookmark_offset + i;
        if manga_idx >= bookmarked.len() {
            break;
        }
        let manga = &bookmarked[manga_idx];
        draw_manga_card(
            f,
            *card_area,
            manga,
            i == 0,
            app.image_states.get_mut(&manga.id),
        );
    }

    // Scroll indicators
    if app.bookmark_offset > 0 {
        let left = Paragraph::new("◀").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(left, Rect::new(inner.x, inner.y + inner.height / 2, 1, 1));
    }
    if app.bookmark_offset + cards_visible < bookmarked.len() {
        let right = Paragraph::new("▶").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(right, Rect::new(inner.x + inner.width - 1, inner.y + inner.height / 2, 1, 1));
    }
}

fn draw_search_content(f: &mut Frame, area: Rect, app: &mut App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search input
            Constraint::Min(5),    // results
        ])
        .split(area);

    // Search input
    let search_style = if app.focus == Focus::Header {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let cursor = if app.focus == Focus::Header { "▌" } else { "" };
    let search_text = format!("🔍 {}{}", app.search_query, cursor);
    
    let search_input = Paragraph::new(search_text)
        .style(search_style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Search Manga")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    f.render_widget(search_input, layout[0]);

    // Results
    let results_block = Block::default()
        .borders(Borders::ALL)
        .title(if app.searching {
            "Searching...".to_string()
        } else {
            format!("Results ({})", app.search_results.len())
        })
        .border_style(Style::default().fg(Color::Yellow));

    let inner = results_block.inner(layout[1]);
    f.render_widget(results_block, layout[1]);

    if app.searching {
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame_idx = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
            / 100) as usize
            % spinner_frames.len();
        let loading = Paragraph::new(format!("{} Searching...", spinner_frames[frame_idx]))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, inner);
        return;
    }

    if app.search_results.is_empty() {
        let msg = if app.search_query.is_empty() {
            "Type a manga name and press Enter to search"
        } else {
            "No results found"
        };
        let empty = Paragraph::new(msg)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(empty, inner);
        return;
    }

    // Clamp offset
    let max_offset = app.search_results.len().saturating_sub(1);
    if app.search_offset > max_offset {
        app.search_offset = max_offset;
    }

    let available_width = inner.width as usize;
    let cards_visible = (available_width / CARD_WIDTH as usize).max(1);

    let card_constraints: Vec<Constraint> = (0..cards_visible)
        .map(|_| Constraint::Length(CARD_WIDTH))
        .collect();

    let card_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(card_constraints)
        .split(inner);

    for (i, card_area) in card_areas.iter().enumerate() {
        let manga_idx = app.search_offset + i;
        if manga_idx >= app.search_results.len() {
            break;
        }
        let manga = &app.search_results[manga_idx];
        draw_manga_card(
            f,
            *card_area,
            manga,
            i == 0,
            app.image_states.get_mut(&manga.id),
        );
    }

    // Scroll indicators
    if app.search_offset > 0 {
        let left = Paragraph::new("◀").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(left, Rect::new(inner.x, inner.y + inner.height / 2, 1, 1));
    }
    if app.search_offset + cards_visible < app.search_results.len() {
        let right = Paragraph::new("▶").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(right, Rect::new(inner.x + inner.width - 1, inner.y + inner.height / 2, 1, 1));
    }
}

fn draw_manga_detail(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let manga = match &app.selected_manga {
        Some(m) => m,
        None => return,
    };

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(10),    // content
            Constraint::Length(3),  // footer
        ])
        .split(area);

    // Header with manga title and bookmark indicator
    let bookmark_indicator = if app.is_current_bookmarked() {
        " ★ Bookmarked"
    } else {
        ""
    };
    let header_text = format!("{}{}", manga.title, bookmark_indicator);
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Manga Details")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    f.render_widget(header, root[0]);

    // Content: manga info + chapters list
    let content_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(40), // manga info
            Constraint::Min(20),    // chapters list
        ])
        .split(root[1]);

    // Manga info panel
    let info_block = Block::default()
        .borders(Borders::ALL)
        .title("Info")
        .border_style(Style::default().fg(Color::Yellow));

    let info_inner = info_block.inner(content_layout[0]);
    f.render_widget(info_block, content_layout[0]);

    let info_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // cover image
            Constraint::Min(5),     // details
        ])
        .split(info_inner);

    // Cover image
    if let Some(state) = app.image_states.get_mut(&manga.id) {
        let image_widget = StatefulImage::new().resize(Resize::Fit(None));
        f.render_stateful_widget(image_widget, info_layout[0], state);
    } else {
        let placeholder = Paragraph::new("📚 Loading cover...")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(placeholder, info_layout[0]);
    }

    // Manga details
    let details = vec![
        Line::from(vec![
            Span::styled("Author: ", Style::default().fg(Color::Yellow)),
            Span::raw(&manga.author),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::Yellow)),
            Span::styled(&manga.status, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(Span::styled("Description:", Style::default().fg(Color::Yellow))),
        Line::from(truncate_text(&manga.description, 35)),
    ];
    let details_paragraph = Paragraph::new(details);
    f.render_widget(details_paragraph, info_layout[1]);

    // Chapters panel with 2D grid
    let chapters_block = Block::default()
        .borders(Borders::ALL)
        .title(format!("Chapters ({}) ←↑↓→ to navigate", app.chapters.len()))
        .border_style(Style::default().fg(Color::Yellow));

    let chapters_inner = chapters_block.inner(content_layout[1]);
    f.render_widget(chapters_block, content_layout[1]);

    if app.chapters.is_empty() {
        let loading = Paragraph::new("Loading chapters...")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(loading, chapters_inner);
    } else {
        // Calculate grid dimensions
        const CHAPTER_CARD_WIDTH: u16 = 22;
        const CHAPTER_CARD_HEIGHT: u16 = 12;
        
        let cols = (chapters_inner.width / CHAPTER_CARD_WIDTH).max(1) as usize;
        let rows = (chapters_inner.height / CHAPTER_CARD_HEIGHT).max(1) as usize;
        
        // Store cols for navigation
        app.chapter_grid_cols = cols;
        
        // Clamp selection
        let max_idx = app.chapters.len().saturating_sub(1);
        if app.chapter_selected > max_idx {
            app.chapter_selected = max_idx;
        }
        
        // Calculate which row the selected chapter is in
        let selected_row = app.chapter_selected / cols;
        
        // Adjust scroll to keep selection visible
        if selected_row < app.chapter_scroll_row {
            app.chapter_scroll_row = selected_row;
        } else if selected_row >= app.chapter_scroll_row + rows {
            app.chapter_scroll_row = selected_row - rows + 1;
        }
        
        // Create row layout
        let row_constraints: Vec<Constraint> = (0..rows)
            .map(|_| Constraint::Length(CHAPTER_CARD_HEIGHT))
            .collect();
        
        let row_areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(chapters_inner);
        
        // Render each row
        for (row_idx, row_area) in row_areas.iter().enumerate() {
            let actual_row = app.chapter_scroll_row + row_idx;
            let start_idx = actual_row * cols;
            
            if start_idx >= app.chapters.len() {
                break;
            }
            
            // Create column layout for this row
            let col_constraints: Vec<Constraint> = (0..cols)
                .map(|_| Constraint::Length(CHAPTER_CARD_WIDTH))
                .collect();
            
            let col_areas = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(col_constraints)
                .split(*row_area);
            
            for (col_idx, col_area) in col_areas.iter().enumerate() {
                let chapter_idx = start_idx + col_idx;
                if chapter_idx >= app.chapters.len() {
                    break;
                }
                
                let chapter = &app.chapters[chapter_idx];
                let is_selected = chapter_idx == app.chapter_selected;
                
                draw_chapter_card(
                    f,
                    *col_area,
                    chapter,
                    is_selected,
                    app.chapter_thumbnails.get_mut(&chapter.id),
                );
            }
        }
        
        // Scroll indicators
        if app.chapter_scroll_row > 0 {
            let up = Paragraph::new("▲ more")
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center);
            f.render_widget(up, Rect::new(chapters_inner.x, chapters_inner.y, chapters_inner.width, 1));
        }
        
        let total_rows = (app.chapters.len() + cols - 1) / cols;
        if app.chapter_scroll_row + rows < total_rows {
            let down = Paragraph::new("▼ more")
                .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center);
            f.render_widget(down, Rect::new(chapters_inner.x, chapters_inner.y + chapters_inner.height - 1, chapters_inner.width, 1));
        }
    }

    let bookmark_hint = if app.is_current_bookmarked() {
        "b: unbookmark"
    } else {
        "b: bookmark"
    };
    draw_footer(f, root[2], &format!("←/→: navigate | Enter: read | {} | Esc: back | q: quit", bookmark_hint));
}

fn draw_reader(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(10),   // page content
            Constraint::Length(3), // footer
        ])
        .split(area);

    // Header with chapter info
    let chapter_info = if let Some(chapter) = app.reader.chapters.get(app.reader.current_chapter_idx) {
        format!(
            "Chapter {} - {} | Page {}/{}",
            chapter.chapter,
            chapter.title,
            app.reader.current_page + 1,
            app.reader.page_urls.len().max(1)
        )
    } else {
        "Loading...".to_string()
    };

    let header = Paragraph::new(chapter_info)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Reader")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    f.render_widget(header, root[0]);

    // Page content
    let content_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = content_block.inner(root[1]);
    f.render_widget(content_block, root[1]);

    if app.reader.loading {
        let loading = Paragraph::new("⏳ Loading page...")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(loading, inner);
    } else if let Some(ref error) = app.reader.error {
        let error_text = Paragraph::new(error.as_str())
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red));
        f.render_widget(error_text, inner);
    } else if let Some(ref mut state) = app.reader.page_image {
        let image_widget = StatefulImage::new().resize(Resize::Fit(None));
        f.render_stateful_widget(image_widget, inner, state);
    } else {
        let error = Paragraph::new("No page to display")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(error, inner);
    }

    let footer_hint = if app.reader.error.is_some() {
        "←/→: page | n: next ch | p: prev ch | r: retry | Esc: back | q: quit"
    } else {
        "←/→: page | n: next ch | p: prev ch | Esc: back | q: quit"
    };
    draw_footer(f, root[2], footer_hint);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let titles = vec!["Home", "Bookmarks", "Search"];
    let selected = match app.tab {
        Tab::Home => 0,
        Tab::Bookmarks => 1,
        Tab::Search => 2,
    };

    let header_style = if app.focus == Focus::Header {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Manga Reader")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .select(selected)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(header_style);

    f.render_widget(tabs, area);
}

fn draw_manga_section(
    f: &mut Frame,
    area: Rect,
    title: &str,
    mangas: &[Manga],
    offset: &mut usize,
    focused: bool,
    image_states: &mut HashMap<String, StatefulProtocol>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    if mangas.is_empty() {
        let loading = Paragraph::new("No manga available")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(loading, inner);
        return;
    }

    // Clamp offset
    let max_offset = mangas.len().saturating_sub(1);
    if *offset > max_offset {
        *offset = max_offset;
    }

    // Calculate how many cards fit
    let available_width = inner.width as usize;
    let cards_visible = (available_width / CARD_WIDTH as usize).max(1);

    // Draw manga cards horizontally
    let card_constraints: Vec<Constraint> = (0..cards_visible)
        .map(|_| Constraint::Length(CARD_WIDTH))
        .collect();

    let card_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(card_constraints)
        .split(inner);

    for (i, card_area) in card_areas.iter().enumerate() {
        let manga_idx = *offset + i;
        if manga_idx >= mangas.len() {
            break;
        }
        let manga = &mangas[manga_idx];
        draw_manga_card(
            f,
            *card_area,
            manga,
            focused && i == 0,
            image_states.get_mut(&manga.id),
        );
    }

    // Draw scroll indicators
    if *offset > 0 {
        let left_indicator = Paragraph::new("◀").style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let left_area = Rect::new(inner.x, inner.y + inner.height / 2, 1, 1);
        f.render_widget(left_indicator, left_area);
    }

    if *offset + cards_visible < mangas.len() {
        let right_indicator = Paragraph::new("▶").style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let right_area = Rect::new(
            inner.x + inner.width.saturating_sub(1),
            inner.y + inner.height / 2,
            1,
            1,
        );
        f.render_widget(right_indicator, right_area);
    }
}

fn draw_manga_card(
    f: &mut Frame,
    area: Rect,
    manga: &Manga,
    selected: bool,
    image_state: Option<&mut StatefulProtocol>,
) {
    let border_style = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 4 || inner.width < 5 {
        return;
    }

    // Layout: image, title, description, rating
    let card_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // image (larger for cover)
            Constraint::Length(2), // title
            Constraint::Min(2),    // description
            Constraint::Length(1), // rating/status
        ])
        .split(inner);

    // Render cover image or placeholder
    if let Some(state) = image_state {
        let image_widget = StatefulImage::new().resize(Resize::Scale(None));
        f.render_stateful_widget(image_widget, card_layout[0], state);
    } else {
        // Placeholder when image not loaded
        let image_content = vec![
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled("📚", Style::default().fg(Color::Magenta))),
            Line::from(Span::styled(
                "Loading...",
                Style::default().fg(Color::DarkGray),
            )),
        ];
        let image_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let image_paragraph = Paragraph::new(image_content)
            .block(image_block)
            .alignment(Alignment::Center);
        f.render_widget(image_paragraph, card_layout[0]);
    }

    // Title (truncated)
    let title = truncate_text(&manga.title, (inner.width.saturating_sub(2)) as usize);
    let title_paragraph = Paragraph::new(title)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Left);
    f.render_widget(title_paragraph, card_layout[1]);

    // Description (truncated, multi-line)
    let desc_width = inner.width.saturating_sub(1) as usize;
    let max_desc_lines = card_layout[2].height.saturating_sub(0) as usize;
    let desc_lines = wrap_text(&manga.description, desc_width, max_desc_lines.max(1));
    let desc_paragraph =
        Paragraph::new(desc_lines.join("\n")).style(Style::default().fg(Color::DarkGray));
    f.render_widget(desc_paragraph, card_layout[2]);

    // Rating/Status line
    let rating_line = Line::from(vec![
        Span::styled("★ ", Style::default().fg(Color::Yellow)),
        Span::styled(&manga.status, Style::default().fg(Color::Cyan)),
    ]);
    let rating_paragraph = Paragraph::new(rating_line);
    f.render_widget(rating_paragraph, card_layout[3]);
}

fn draw_chapter_card(
    f: &mut Frame,
    area: Rect,
    chapter: &Chapter,
    selected: bool,
    image_state: Option<&mut StatefulProtocol>,
) {
    let border_style = if selected {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 4 || inner.width < 5 {
        return;
    }

    // Layout: image, chapter number, title, pages
    let card_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),    // image
            Constraint::Length(1), // chapter number
            Constraint::Length(2), // title
            Constraint::Length(1), // pages
        ])
        .split(inner);

    // Render cover image or placeholder
    if let Some(state) = image_state {
        let image_widget = StatefulImage::new().resize(Resize::Fit(None));
        f.render_stateful_widget(image_widget, card_layout[0], state);
    } else if chapter.external_url.is_some() {
        let placeholder = Paragraph::new("🔗\nExternal")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Magenta));
        f.render_widget(placeholder, card_layout[0]);
    } else {
        let placeholder = Paragraph::new("📖\nLoading...")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(placeholder, card_layout[0]);
    }

    // Chapter number
    let vol = chapter.volume.as_ref().map(|v| format!("V{} ", v)).unwrap_or_default();
    let chapter_num = format!("{}Ch.{}", vol, chapter.chapter);
    let chapter_paragraph = Paragraph::new(truncate_text(&chapter_num, inner.width as usize))
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center);
    f.render_widget(chapter_paragraph, card_layout[1]);

    // Title (truncated)
    let title = if chapter.title.is_empty() {
        "Untitled".to_string()
    } else {
        chapter.title.clone()
    };
    let title_lines = wrap_text(&title, inner.width as usize, 2);
    let title_paragraph = Paragraph::new(title_lines.join("\n"))
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center);
    f.render_widget(title_paragraph, card_layout[2]);

    // Pages
    let pages_text = format!("{} pages", chapter.pages);
    let pages_paragraph = Paragraph::new(pages_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(pages_paragraph, card_layout[3]);
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        format!(
            "{}...",
            text.chars()
                .take(max_len.saturating_sub(3))
                .collect::<String>()
        )
    }
}

fn wrap_text(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return vec![];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.chars().count() + 1 + word.chars().count() <= width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            if lines.len() >= max_lines {
                if let Some(last) = lines.last_mut() {
                    let char_count = last.chars().count();
                    if char_count > 3 {
                        *last = last.chars().take(char_count - 3).collect::<String>() + "...";
                    }
                }
                return lines;
            }
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() && lines.len() < max_lines {
        lines.push(current_line);
    }

    lines
}

fn draw_footer(f: &mut Frame, area: Rect, help_text: &str) {
    let spans: Vec<Span> = help_text
        .split(" | ")
        .flat_map(|part| {
            let mut parts = part.splitn(2, ": ");
            if let (Some(key), Some(desc)) = (parts.next(), parts.next()) {
                vec![
                    Span::styled(key, Style::default().fg(Color::Yellow)),
                    Span::raw(": "),
                    Span::raw(desc),
                    Span::raw("  "),
                ]
            } else {
                vec![Span::raw(part), Span::raw("  ")]
            }
        })
        .collect();

    let text = Line::from(spans);

    let p = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}
