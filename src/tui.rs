use crate::connection::{ConnectionStatus, ForwardBinding, StatusEvent};
use crate::shutdown::Shutdown;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Terminal;
use std::collections::BTreeMap;
use std::io::{stdout, Stdout};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

pub fn run(receiver: Receiver<StatusEvent>, shutdown: Shutdown) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;
    let result = run_loop(&mut terminal, receiver, shutdown.clone());
    restore_terminal(terminal)?;
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    receiver: Receiver<StatusEvent>,
    shutdown: Shutdown,
) -> Result<()> {
    let mut state = UiState::default();
    loop {
        loop {
            match receiver.try_recv() {
                Ok(event) => state.apply(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    state.disconnected = true;
                    break;
                }
            }
        }
        terminal.draw(|frame| {
            let area = frame.size();
            let chunks =
                Layout::new(Direction::Vertical, [Constraint::Percentage(100)]).split(area);
            if state.entries.is_empty() {
                let text = if state.disconnected {
                    "No active tasks"
                } else {
                    "Waiting for updates…"
                };
                let paragraph = Paragraph::new(text)
                    .block(Block::default().borders(Borders::ALL).title("ssht"));
                frame.render_widget(paragraph, chunks[0]);
            } else {
                let header = Row::new(vec![
                    Cell::from("Namespace"),
                    Cell::from("Status"),
                    Cell::from("Attempt"),
                    Cell::from("Ports"),
                    Cell::from("Message"),
                ])
                .style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
                let widths = [
                    Constraint::Length(24),
                    Constraint::Length(16),
                    Constraint::Length(8),
                    Constraint::Length(20),
                    Constraint::Min(10),
                ];
                let rows = state.entries.iter().map(|(name, entry)| {
                    let (text, color) = status_view(&entry.status);
                    Row::new(vec![
                        Cell::from(name.clone()),
                        Cell::from(text).style(Style::default().fg(color)),
                        Cell::from(entry.attempt.to_string()),
                        Cell::from(entry.ports.clone()),
                        Cell::from(entry.message.clone().unwrap_or_default()),
                    ])
                });
                let table = Table::new(rows, widths)
                    .header(header)
                    .block(Block::default().borders(Borders::ALL).title("ssht"));
                frame.render_widget(table, chunks[0]);
            }
        })?;
        if shutdown.is_triggered() {
            break;
        }
        if state.disconnected && state.entries.is_empty() {
            break;
        }
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        shutdown.trigger();
                        break;
                    }
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                        shutdown.trigger();
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    terminal.show_cursor()?;
    disable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, LeaveAlternateScreen)?;
    Ok(())
}

#[derive(Default)]
struct UiState {
    entries: BTreeMap<String, UiEntry>,
    disconnected: bool,
}

impl UiState {
    fn apply(&mut self, event: StatusEvent) {
        let entry = self
            .entries
            .entry(event.namespace.clone())
            .or_insert_with(|| UiEntry {
                status: ConnectionStatus::Connecting,
                attempt: 0,
                message: None,
                ports: format_ports(&event.forwards),
            });
        entry.status = event.status;
        entry.attempt = event.attempt;
        entry.message = event.message;
        entry.ports = format_ports(&event.forwards);
    }
}

struct UiEntry {
    status: ConnectionStatus,
    attempt: u32,
    message: Option<String>,
    ports: String,
}

fn status_view(status: &ConnectionStatus) -> (&'static str, Color) {
    match status {
        ConnectionStatus::Connecting => ("Connecting", Color::Yellow),
        ConnectionStatus::Connected => ("Connected", Color::Green),
        ConnectionStatus::Reconnecting => ("Retrying", Color::Magenta),
        ConnectionStatus::Failed => ("Failed", Color::Red),
        ConnectionStatus::Stopped => ("Stopped", Color::Gray),
    }
}

fn format_ports(forwards: &[ForwardBinding]) -> String {
    if forwards.is_empty() {
        return String::from("-");
    }
    forwards
        .iter()
        .map(|forward| format!("{} -> {}", forward.local, forward.remote))
        .collect::<Vec<_>>()
        .join(", ")
}
