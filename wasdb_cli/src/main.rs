use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::{error::Error, io};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the database file
    #[arg(short, long, default_value = "wasdb.db")]
    db_path: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Setup Ratatui terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run TUI
    let res = run_app(&mut terminal, &args.db_path);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, db_path: &str) -> io::Result<()> {
    let mut input = String::new();
    let mut logs = vec![format!("Connected to database at: {}", db_path)];

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
                .split(f.size());

            let input_widget = Paragraph::new(format!("> {}", input))
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().borders(Borders::ALL).title("SQL Input"));
            f.render_widget(input_widget, chunks[0]);

            let messages: String = logs.join("\n");
            let logs_widget = Paragraph::new(messages)
                .block(Block::default().borders(Borders::ALL).title("Output"));
            f.render_widget(logs_widget, chunks[1]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char(c) => {
                    input.push(c);
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter => {
                    if input.trim() == "exit" || input.trim() == "quit" {
                        return Ok(());
                    }

                    logs.push(format!("Executing: {}", input));
                    // Basic mock execution
                    logs.push(String::from("Success. (0 rows affected)"));

                    input.clear();
                }
                KeyCode::Esc => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}
