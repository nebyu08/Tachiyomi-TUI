mod ui;
use ui::ui::App;
use ui::ui::ui;

use std::{error::Error, io, time::Duration};
use crossterm::{
event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
execute,
terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
backend::CrosstermBackend, Terminal,
};

// #[derive(Default)]
// pub struct App {
// search: String,
// recent_offset: usize,
// popular_offset: usize,
// focus: Focus,
// recently_updated: Vec<String>,
// popular_now: Vec<String>,
// }

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    Recent,
    Popular,
}

// impl Default for Focus {
//     fn default() -> Self { Focus::Search }
// }
fn main() ->Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    
    let mut app = App {
    recently_updated: (1..=20).map(|i| format!("Recent #{i}")).collect(),
    popular_now: (1..=20).map(|i| format!("Popular #{i}")).collect(),
    ..Default::default()
    };
    
    
    let res = run_app(&mut terminal, &mut app);
    
    
    // restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    
    
    if let Err(err) = res {
    eprintln!("{err}");
    }
    Ok(())
    
}

fn run_app<B: ratatui::backend::Backend>(
terminal: &mut Terminal<B>,
app: &mut App,
) -> io::Result<()> {
loop {
    terminal.draw(|f| ui(f, app))?;
    
    if event::poll(Duration::from_millis(200))? {
        if let Event::Key(key) = event::read()? {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Tab => {
                app.focus = match app.focus {
                Focus::Search => Focus::Recent,
                Focus::Recent => Focus::Popular,
                Focus::Popular => Focus::Search,
            }
            }
            KeyCode::Left => match app.focus {
                Focus::Recent => app.recent_offset = app.recent_offset.saturating_sub(1),
                Focus::Popular => app.popular_offset = app.popular_offset.saturating_sub(1),
                _ => {}
            },
            KeyCode::Right => match app.focus {
                Focus::Recent => app.recent_offset += 1,
                Focus::Popular => app.popular_offset += 1,
                _ => {}
            },
            KeyCode::Backspace => {
                if app.focus == Focus::Search {
                app.search.pop();
                }
            }
            KeyCode::Char(c) => {
                if app.focus == Focus::Search {
                app.search.push(c);
                }
            }
            _ => {}
        }
        }
    }
}
}