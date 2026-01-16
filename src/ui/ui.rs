use image::DynamicImage;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, Resize, StatefulImage};
use std::collections::HashMap;

use crate::backend::mangadex::Manga;

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

pub struct App {
    pub state: AppState,
    pub loading_message: String,
    pub tab: Tab,
    pub focus: Focus,
    pub search_query: String,
    pub recent_offset: usize,
    pub popular_offset: usize,
    pub recently_updated: Vec<Manga>,
    pub popular_now: Vec<Manga>,
    pub picker: Option<Picker>,
    pub cover_images: HashMap<String, DynamicImage>,
    pub image_states: HashMap<String, StatefulProtocol>,
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
            loading_message: "Initializing...".to_string(),
            tab: Tab::Home,
            focus: Focus::Header,
            search_query: String::new(),
            recent_offset: 0,
            popular_offset: 0,
            recently_updated: Vec::new(),
            popular_now: Vec::new(),
            picker,
            cover_images: HashMap::new(),
            image_states: HashMap::new(),
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
}

const CARD_WIDTH: u16 = 35;

pub fn ui(f: &mut Frame, app: &mut App) {
    match app.state {
        AppState::Loading => draw_loading_screen(f, app),
        AppState::Ready => draw_main_ui(f, app),
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

    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50), // recently updated
            Constraint::Percentage(50), // popular now
        ])
        .split(root[1]);

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

    draw_footer(f, root[2]);
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

fn draw_footer(f: &mut Frame, area: Rect) {
    let text = Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": switch section  "),
        Span::styled("←/→", Style::default().fg(Color::Yellow)),
        Span::raw(": scroll  "),
        Span::styled("↑/↓", Style::default().fg(Color::Yellow)),
        Span::raw(": focus  "),
        Span::styled("q", Style::default().fg(Color::Yellow)),
        Span::raw(": quit"),
    ]);

    let p = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}
