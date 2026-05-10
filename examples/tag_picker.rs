use std::io;

use ratatui::{
    Frame,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use ratatui_tag_picker::{TagPicker, TagPickerConfig, TagPickerState};

fn main() -> io::Result<()> {
    ratatui::run(|terminal| {
        let mut app = App::new();

        loop {
            terminal.draw(|frame| app.render(frame))?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if app.handle_key(key.code) {
                    break Ok(());
                }
            }
        }
    })
}

struct App {
    picker: TagPicker,
    picker_state: TagPickerState,
}

impl App {
    fn new() -> Self {
        let picker = TagPicker::with_config(
            [
                "backend",
                "bug",
                "cli",
                "docs",
                "frontend",
                "help wanted",
                "high priority",
                "low priority",
                "ratatui",
                "rust",
                "testing",
                "ui",
                "ux",
                "wip",
            ],
            TagPickerConfig {
                input_height: 5,
                accent_color: Color::Green,
            },
        );
        let picker_state =
            TagPickerState::new_with_selected_tags(&picker, ["ratatui", "rust", "ui"]);

        Self {
            picker,
            picker_state,
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('q') => return true,
            KeyCode::Tab => {
                self.picker_state.cycle_focus();
                return false;
            }
            _ => {}
        }

        self.handle_key_action(code);

        false
    }

    fn handle_key_action(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => {
                self.picker_state.confirm(&self.picker);
            }
            KeyCode::Backspace => {
                self.picker_state.backspace();
                self.picker_state.remove_selected_tag(&self.picker);
            }
            KeyCode::Delete => {
                self.picker_state.remove_selected_tag(&self.picker);
            }
            KeyCode::Up | KeyCode::Left => {
                self.picker_state.move_previous(&self.picker);
            }
            KeyCode::Down | KeyCode::Right => {
                self.picker_state.move_next(&self.picker);
            }
            KeyCode::Char(ch) => {
                if ch == 'd' {
                    self.picker_state.remove_selected_tag(&self.picker);
                } else {
                    self.picker_state.insert_char(ch);
                }
            }
            _ => {}
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let vertical = Layout::vertical([Constraint::Min(10), Constraint::Length(4)]).split(area);

        frame.render_stateful_widget(&self.picker, vertical[0], &mut self.picker_state);

        let help = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Global", Style::new().fg(Color::Yellow)),
                Span::raw(": "),
                Span::raw("Tab switch focus, q quit"),
            ]),
            Line::from(vec![
                Span::styled("Input", Style::new().fg(Color::Yellow)),
                Span::raw(": "),
                Span::raw("type query, arrows navigate dropdown, Enter add"),
            ]),
            Line::from(vec![
                Span::styled("Selected", Style::new().fg(Color::Yellow)),
                Span::raw(": "),
                Span::raw("arrows move selection, d/Delete/Backspace remove"),
            ]),
        ])
        .block(Block::default().borders(Borders::ALL).title("Controls"))
        .wrap(Wrap { trim: false });
        frame.render_widget(help, vertical[1]);
    }
}
