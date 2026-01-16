mod backend;
mod ui;

use backend::mangadex::{get_popular_now, get_recently_updated};
use ui::ui::{App, Focus, Tab, ui};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{error::Error, io, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    // Show loading screen
    app.set_loading("Connecting to MangaDex...");
    terminal.draw(|f| ui(f, &mut app))?;

    // Fetch data with loading updates
    app.set_loading("Fetching recently updated manga...");
    terminal.draw(|f| ui(f, &mut app))?;
    
    if let Ok(recent) = get_recently_updated().await {
        app.recently_updated = recent;
    }

    app.set_loading("Fetching popular manga...");
    terminal.draw(|f| ui(f, &mut app))?;

    if let Ok(popular) = get_popular_now().await {
        app.popular_now = popular;
    }

    // Data loaded, switch to ready state
    app.set_ready();

    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("{err}");
    }
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => return Ok(()),

                    // Switch focus between sections (Up/Down or Tab)
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

                    // Horizontal scrolling in manga sections
                    KeyCode::Left => match app.focus {
                        Focus::Header => {
                            app.tab = match app.tab {
                                Tab::Home => Tab::Search,
                                Tab::Bookmarks => Tab::Home,
                                Tab::Search => Tab::Bookmarks,
                            }
                        }
                        Focus::Recent => {
                            app.recent_offset = app.recent_offset.saturating_sub(1)
                        }
                        Focus::Popular => {
                            app.popular_offset = app.popular_offset.saturating_sub(1)
                        }
                    },
                    KeyCode::Right => match app.focus {
                        Focus::Header => {
                            app.tab = match app.tab {
                                Tab::Home => Tab::Bookmarks,
                                Tab::Bookmarks => Tab::Search,
                                Tab::Search => Tab::Home,
                            }
                        }
                        Focus::Recent => app.recent_offset += 1,
                        Focus::Popular => app.popular_offset += 1,
                    },

                    // Search input when in search tab
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
    }
}
