use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;

use crate::tools::multi_select::SelectableItem;

/// Multi-select application state for TUI
pub struct MultiSelectApp {
    descriptions: Vec<String>,
    details: Vec<Option<String>>,
    ids: Vec<String>,
    state: ListState,
    selected_indices: Vec<usize>,
    show_detail: bool,
}

impl MultiSelectApp {
    pub fn new(items: &[SelectableItem], _question: &str) -> Self {
        let descriptions: Vec<String> = items
            .iter()
            .map(|i| i.description.clone())
            .collect();
        
        let details: Vec<Option<String>> = items
            .iter()
            .map(|i| i.detail.clone())
            .collect();
        
        let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();

        let mut state = ListState::default();
        if !ids.is_empty() {
            state.select(Some(0));
        }

        Self {
            descriptions,
            details,
            ids,
            state,
            selected_indices: Vec::new(),
            show_detail: false,
        }
    }

    pub fn run(mut self, question: &str) -> Result<Vec<String>, String> {
        enable_raw_mode().map_err(|e| e.to_string())?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| e.to_string())?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

        loop {
            terminal.draw(|f| self.ui(f, question)).map_err(|e| e.to_string())?;

            if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            cleanup_terminal(&mut terminal)?;
                            return Ok(Vec::new());
                        }
                        KeyCode::Up => {
                            if self.ids.is_empty() {
                                continue;
                            }
                            let i = match self.state.selected() {
                                Some(i) => {
                                    if i == 0 {
                                        self.ids.len() - 1
                                    } else {
                                        i - 1
                                    }
                                }
                                None => 0,
                            };
                            self.state.select(Some(i));
                        }
                        KeyCode::Down => {
                            if self.ids.is_empty() {
                                continue;
                            }
                            let i = match self.state.selected() {
                                Some(i) => {
                                    if i >= self.ids.len() - 1 {
                                        0
                                    } else {
                                        i + 1
                                    }
                                }
                                None => 0,
                            };
                            self.state.select(Some(i));
                        }
                        KeyCode::Char(' ') => {
                            if let Some(i) = self.state.selected() {
                                if self.selected_indices.contains(&i) {
                                    self.selected_indices.retain(|&x| x != i);
                                } else {
                                    self.selected_indices.push(i);
                                }
                            }
                        }
                        KeyCode::Char('v') => {
                            // Toggle showing the detail
                            self.show_detail = !self.show_detail;
                        }
                        KeyCode::Char('h') => {
                            // Hide detail
                            self.show_detail = false;
                        }
                        KeyCode::Char('a') => {
                            // Select/deselect all items
                            if self.ids.is_empty() {
                                continue;
                            }
                            if self.selected_indices.len() == self.ids.len() {
                                // All selected, deselect all
                                self.selected_indices.clear();
                            } else {
                                // Select all
                                self.selected_indices = (0..self.ids.len()).collect();
                            }
                        }
                        KeyCode::Enter => {
                            cleanup_terminal(&mut terminal)?;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }

        cleanup_terminal(&mut terminal)?;
        
        // Convert selected indices to item IDs
        let selected_ids: Vec<String> = self.selected_indices
            .iter()
            .map(|&i| self.ids[i].clone())
            .collect();
        
        Ok(selected_ids)
    }

    fn ui(&mut self, f: &mut Frame, question: &str) {
        let constraints = if self.show_detail {
            vec![Constraint::Length(5), Constraint::Length(10), Constraint::Min(3)]
        } else {
            vec![Constraint::Length(5), Constraint::Min(3)]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(f.area());

        let header_text = format!("{} | Space: toggle, A: select all, V: view details, H: hide, Enter: confirm, q: quit", question);
        let header = Paragraph::new(header_text.as_str())
            .style(Style::default().fg(Color::White))
            .block(Block::default().borders(Borders::ALL).title("Options"));
        f.render_widget(header, chunks[0]);

        // Show detail if enabled
        if self.show_detail {
            if let Some(selected_idx) = self.state.selected() {
                let detail_text = match self.details.get(selected_idx).and_then(|d| d.as_ref()) {
                    Some(detail) => format!("Details:\n{}", detail),
                    None => "No details available".to_string(),
                };
                let detail_paragraph = Paragraph::new(detail_text.as_str())
                    .style(Style::default().fg(Color::Yellow))
                    .block(Block::default().borders(Borders::ALL).title("Item Details"))
                    .wrap(ratatui::widgets::Wrap { trim: true });
                f.render_widget(detail_paragraph, chunks[1]);
            }
        }

        // Display description with checkbox
        let styled_items: Vec<ListItem> = self.descriptions.iter().enumerate().map(|(i, desc)| {
            let is_selected = self.selected_indices.contains(&i);
            let is_highlighted = self.state.selected() == Some(i);
            
            // Create checkbox symbol based on selection
            let checkbox = if is_selected { "☑" } else { "☐" };
            
            // Determine style
            let style = if is_highlighted {
                if is_selected {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                }
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            
            ListItem::new(format!("{} {}", checkbox, desc)).style(style)
        }).collect();

        let list = List::new(styled_items)
            .block(Block::default().borders(Borders::ALL).title("Available Options"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        
        let list_chunk = if self.show_detail { chunks[2] } else { chunks[1] };
        f.render_stateful_widget(list, list_chunk, &mut self.state);
    }
}

fn cleanup_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), String> {
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    ).map_err(|e| e.to_string())?;
    disable_raw_mode().map_err(|e| e.to_string())?;
    Ok(())
}
