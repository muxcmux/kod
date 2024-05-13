use std::fmt::Display;

use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}, style::Color};
use unicode_segmentation::UnicodeSegmentation;
use crate::{commands::COMMANDS, compositor::{Component, Context, EventResult}, editor::Mode, ui::{Buffer, Position, Rect}};

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

    fn run(&mut self, ctx: &mut Context) {
        for cmd in COMMANDS {
            if cmd.name == self.text || cmd.aliases.contains(&self.text.as_str()) {
                (cmd.func)(ctx);
                break;
            }
        }
        self.dismiss();
    }

    fn update_command(&mut self, key_code: KeyCode, ctx: &mut Context) {
        match key_code {
            // Need to somehow merge this with the insert mode keymap
            // so that we get consistent editing text experience
            KeyCode::Char(c) => self.text.push(c),
            KeyCode::Esc => self.dismiss(),
            KeyCode::Enter => self.run(ctx),
            _ => {
                //do nothing
            }
        }
    }
}

impl Component for CommandLine {
    fn resize(&mut self, new_size: Rect, _ctx: &mut Context) {
        self.area = new_size.clip_top(new_size.height.saturating_sub(1));
    }

    fn render(&mut self, _area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        if self.focused {
            buffer.put_string(format!("{}", self), self.area.left(), self.area.top(), Color::Reset, Color::Reset);
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
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
                    Some(SetCursorStyle::SteadyBar)
            )
        } else {
            (None, None)
        }
    }
}
