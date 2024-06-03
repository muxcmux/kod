use crate::ui::Position;
use crate::ui::buffer::Buffer;
use crate::ui::Rect;
use std::fmt::Display;

use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}, style::Color};
use unicode_segmentation::UnicodeSegmentation;
use crate::{commands::COMMANDS, compositor::{Component, Context, EventResult}, editor::{EditorStatus, Mode, Severity}};

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

    fn run(&mut self, ctx: &mut Context) -> anyhow::Result<EventResult> {
        for cmd in COMMANDS {
            if cmd.name == self.text || cmd.aliases.contains(&self.text.as_str()) {
                let mut ctx = crate::commands::Context { editor: ctx.editor, compositor_callbacks: vec![]  };

                (cmd.func)(&mut ctx);

                if ctx.compositor_callbacks.is_empty() {
                    return Ok(EventResult::Consumed(None))
                }

                return Ok(EventResult::Consumed(Some(Box::new(move |compositor, cx| {
                    for cb in ctx.compositor_callbacks {
                        cb(compositor, cx);
                    }
                }))));
            }
        }

        Err(anyhow::anyhow!(":{} is not an editor command", self.text))
    }

    fn update_command(&mut self, key_code: KeyCode, ctx: &mut Context) -> EventResult {
        match key_code {
            // Need to somehow merge this with the insert mode keymap
            // so that we get consistent editing text experience
            // maybe have a TextInput component?
            KeyCode::Char(c) => {
                self.text.push(c);
                EventResult::Consumed(None)
            },
            KeyCode::Esc => {
                self.dismiss();
                EventResult::Consumed(None)
            },
            KeyCode::Backspace => {
                self.text.pop();
                EventResult::Consumed(None)
            },
            KeyCode::Enter => {
                let ev = match self.run(ctx) {
                    Ok(result) => result,
                    Err(err) => {
                        ctx.editor.set_error(err.to_string());
                        EventResult::Consumed(None)
                    }
                };
                self.dismiss();
                ev
            }
            _ => EventResult::Ignored(None)
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
                    return self.update_command(event.code, ctx);
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
