use std::fmt::Display;

use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}, style::Color};
use unicode_segmentation::UnicodeSegmentation;
use crate::{commands::COMMANDS, compositor::{Component, Context, EventResult}, editor::{EditorStatus, Mode, Severity}, ui::{Buffer, Position, Rect}};

const PROMPT: &str = ":";

#[derive(Debug)]
pub struct CommandLine {
    area: Rect,
    focused: bool,
    text: String,
}

impl Display for CommandLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", PROMPT, self.text)
    }
}

impl CommandLine {
    pub fn new(area: Rect) -> Self {
        Self { area, text: "".into(), focused: false }
    }

    fn dismiss(&mut self) {
        self.text.clear();
        self.focused = false;
    }

    fn run(&mut self, ctx: &mut Context) -> bool {
        for cmd in COMMANDS {
            if cmd.name == self.text || cmd.aliases.contains(&self.text.as_str()) {
                (cmd.func)(ctx);
                return true;
            }
        }

        false
    }

    fn update_command(&mut self, key_code: KeyCode, ctx: &mut Context) {
        match key_code {
            // Need to somehow merge this with the insert mode keymap
            // so that we get consistent editing text experience
            KeyCode::Char(c) => self.text.push(c),
            KeyCode::Esc => self.dismiss(),
            KeyCode::Enter => {
                if !self.run(ctx) {
                    ctx.editor.set_error(format!(":{} is not an editor command", self.text));
                }
                self.dismiss();
            }
            _ => {
                //do nothing
            }
        }
    }

    fn render_editor_status(&self, status: &EditorStatus, buffer: &mut Buffer) {
        let fg = match status.severity {
            Severity::Error => Color::Red,
            _ => Color::Reset
        };

        buffer.put_string(status.message.to_string(), self.area.left(), self.area.top(), fg, Color::Reset);
    }
}


impl Component for CommandLine {
    fn resize(&mut self, new_size: Rect, _ctx: &mut Context) {
        self.area = new_size.clip_top(new_size.height.saturating_sub(1));
    }

    fn render(&mut self, _area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        if self.focused {
            buffer.put_string(format!("{}", self), self.area.left(), self.area.top(), Color::Reset, Color::Reset);
        } else if let Some(s) = &ctx.editor.status {
            self.render_editor_status(s, buffer);
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        ctx.editor.status = None;

        match ctx.editor.mode {
            Mode::Insert => EventResult::Ignored(None),
            Mode::Normal => {
                if self.focused {
                    self.update_command(event.code, ctx);
                    return EventResult::Consumed(None);
                } else if matches!(event.code, KeyCode::Char(':')) {
                    self.focused = true;
                    return EventResult::Consumed(None);
                }
                EventResult::Ignored(None)
            }
        }
    }

    fn cursor(&self, _area: Rect, _ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        if self.focused {
            let width: usize = self.text.graphemes(true).map(|g| unicode_display_width::width(g) as usize).sum();
            (
                Some(
                    Position {
                        y: self.area.top(),
                        x: self.area.left() + width as u16 + 1
                    }
                    ),
                    Some(SetCursorStyle::SteadyUnderScore)
            )
        } else {
            (None, None)
        }
    }
}
