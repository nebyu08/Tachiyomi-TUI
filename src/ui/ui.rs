use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

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

#[derive(Default)]
pub struct App {
    pub tab: Tab,
    pub focus: Focus,
    pub search_query: String,
    pub recent_offset: usize,
    pub popular_offset: usize,
    pub recently_updated: Vec<Manga>,
    pub popular_now: Vec<Manga>,
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }
}

const CARD_WIDTH: u16 = 30;
const CARD_HEIGHT: u16 = 12;

pub fn ui(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),            // header/tabs
            Constraint::Length(CARD_HEIGHT + 2), // recently updated
            Constraint::Length(CARD_HEIGHT + 2), // popular now
            Constraint::Length(3),            // footer
        ])
        .split(f.area());

    draw_header(f, root[0], app);
    draw_manga_section(
        f,
        root[1],
        "Recently Updated",
        &app.recently_updated,
        &mut app.recent_offset,
        app.focus == Focus::Recent,
    );
    draw_manga_section(
        f,
        root[2],
        "Popular Now",
        &app.popular_now,
        &mut app.popular_offset,
        app.focus == Focus::Popular,
    );
    draw_footer(f, root[3]);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let titles = vec!["Home", "Bookmarks", "Search"];
    let selected = match app.tab {
        Tab::Home => 0,
        Tab::Bookmarks => 1,
        Tab::Search => 2,
    };

    let header_style = if app.focus == Focus::Header {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Manga Reader"))
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
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    if mangas.is_empty() {
        let loading = Paragraph::new("Loading...")
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
        draw_manga_card(f, *card_area, &mangas[manga_idx], focused && i == 0);
    }

    // Draw scroll indicators
    if *offset > 0 {
        let left_indicator = Paragraph::new("◀")
            .style(Style::default().fg(Color::Yellow));
        let left_area = Rect::new(inner.x, inner.y + inner.height / 2, 1, 1);
        f.render_widget(left_indicator, left_area);
    }

    if *offset + cards_visible < mangas.len() {
        let right_indicator = Paragraph::new("▶")
            .style(Style::default().fg(Color::Yellow));
        let right_area = Rect::new(
            inner.x + inner.width.saturating_sub(1),
            inner.y + inner.height / 2,
            1,
            1,
        );
        f.render_widget(right_indicator, right_area);
    }
}

fn draw_manga_card(f: &mut Frame, area: Rect, manga: &Manga, selected: bool) {
    let border_style = if selected {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Layout: image placeholder, title, description, rating
    let card_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // image placeholder
            Constraint::Length(2), // title
            Constraint::Min(3),    // description
            Constraint::Length(1), // rating/status
        ])
        .split(inner);

    // Image placeholder
    let image_block = Block::default()
        .borders(Borders::ALL)
        .title("📖")
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(image_block, card_layout[0]);

    // Title (truncated)
    let title = truncate_text(&manga.title, (inner.width.saturating_sub(2)) as usize);
    let title_paragraph = Paragraph::new(title)
        .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Left);
    f.render_widget(title_paragraph, card_layout[1]);

    // Description (truncated, multi-line)
    let desc_width = inner.width.saturating_sub(2) as usize;
    let desc_lines = wrap_text(&manga.description, desc_width, 2);
    let desc_paragraph = Paragraph::new(desc_lines.join("\n"))
        .style(Style::default().fg(Color::DarkGray));
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
        format!("{}...", text.chars().take(max_len.saturating_sub(3)).collect::<String>())
    }
}

fn wrap_text(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            if lines.len() >= max_lines {
                if let Some(last) = lines.last_mut() {
                    if last.len() > 3 {
                        last.truncate(last.len() - 3);
                        last.push_str("...");
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
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(p, area);
}
