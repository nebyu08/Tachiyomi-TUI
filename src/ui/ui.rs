use ratatui::{
layout::{Alignment, Constraint, Direction, Layout, Rect},
style::{Modifier, Style},
text::{Line, Span},
widgets::{Block, Borders, Paragraph},
Frame,
};

#[derive(Default)]
pub struct App {
search: String,
recent_offset: usize,
popular_offset: usize,
focus: Focus,
recently_updated: Vec<String>,
popular_now: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    Recent,
    Popular,
}

impl Default for Focus {
    fn default() -> Self { Focus::Search }
}

pub fn ui(f: &mut Frame, app: &mut App) {
    let root = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
    Constraint::Length(3), // header
    Constraint::Length(7), // recently updated
    Constraint::Length(7), // popular now
    Constraint::Length(3), // footer
    ])
    .split(f.size());
    
    
    draw_header(f, root[0], app);
    draw_horizontal_list(
    f,
    root[1],
    "Recently Updated",
    &app.recently_updated,
    &mut app.recent_offset,
    app.focus == Focus::Recent,
    );
    draw_horizontal_list(
    f,
    root[2],
    "Popular Now",
    &app.popular_now,
    &mut app.popular_offset,
    app.focus == Focus::Popular,
    );
    draw_footer(f, root[3]);
}

pub fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("Manga Reader");
    let search_style = if app.focus == Focus::Search {
    Style::default().add_modifier(Modifier::BOLD)
    } else {
    Style::default()
    };
    let text = Line::from(vec![
    Span::raw("Search: "),
    Span::styled(&app.search, search_style),
    Span::raw("_")
    ]);
    let p = Paragraph::new(text).block(block);
    f.render_widget(p, area);
}

pub fn draw_horizontal_list(
f: &mut Frame,
area: Rect,
title: &str,
items: &[String],
offset: &mut usize,
focused: bool,
) {
    let block = Block::default()
    .borders(Borders::ALL)
    .title(title)
    .border_style(if focused {
    Style::default().add_modifier(Modifier::BOLD)
    } else {
    Style::default()
    });
    
    
    // Each item rendered as [ Item ] blocks in a single line
    let visible_width = area.width.saturating_sub(2) as usize;
    let mut line = Vec::new();
    
    
    let mut used = 0usize;
    for item in items.iter().skip(*offset) {
    let label = format!("[ {} ] ", item);
    let len = label.len();
    if used + len > visible_width { break; }
    used += len;
    line.push(Span::raw(label));
    }
    
    
    // Clamp offset so we don't scroll past the end
    if *offset > items.len().saturating_sub(1) {
    *offset = items.len().saturating_sub(1);
    }
    
    
    let p = Paragraph::new(Line::from(line))
    .block(block)
    .alignment(Alignment::Left);
    f.render_widget(p, area);
}

pub fn draw_footer(f: &mut Frame, area: Rect) {
    let text = Line::from("Tab: switch focus ←/→: scroll q: quit");
    let p = Paragraph::new(text)
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    f.render_widget(p, area);
}