use crate::{
    compositor::{Component, Context, EventResult}, ui::{
        border_box::BorderBox, borders::{Stroke, Borders}, buffer::Buffer, text_input::TextInput, theme::THEME, Position, Rect
    }
};
use crossterm::{
    cursor::SetCursorStyle,
    event::{KeyCode, KeyEvent},
};

use super::{Command, COMMANDS};

pub struct Pallette {
    input: TextInput,
    index: usize,
}

impl Pallette {
    pub fn new() -> Self {
        Self {
            input: TextInput::empty(),
            index: 0,
        }
    }

    fn run(&mut self, ctx: &mut Context) -> EventResult {
        let idx = self.index;

        if let Some(cmd) = self.commands().get(idx) {
            let mut ctx = crate::commands::Context {
                editor: ctx.editor,
                compositor_callbacks: vec![],
                on_next_key_callback: None,
            };

            (cmd.func)(&mut ctx);

            if ctx.compositor_callbacks.is_empty() {
                return EventResult::Consumed(Some(Box::new(|compositor, _| {
                    compositor.pop();
                })));
            }

            return EventResult::Consumed(Some(Box::new(|compositor, cx| {
                // close the pallette first
                compositor.pop();
                for cb in ctx.compositor_callbacks {
                    cb(compositor, cx);
                }
            })));
        }

        EventResult::Ignored(None)
    }

    fn commands(&mut self) -> Vec<&Command> {
        let text = self.input.value();
        COMMANDS
            .iter()
            .filter(|c| {
                text == "\n" || c.name.contains(&text) || c.aliases.iter().any(|c| *c == text)
            })
            .collect()
    }
}

impl Component for Pallette {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        let size = area.clip_bottom(1).centered(50, 10);

        let bbox = BorderBox::new(size)
            .title("Command")
            .borders(Borders::ALL)
            .style(THEME.get("ui.dialog.border"))
            .stroke(Stroke::Rounded);

        bbox.render(buffer).split_horizontally(2, buffer);

        let inner = bbox.inner();
        let input_size = inner.clip_bottom(inner.height.saturating_sub(1));

        self.input.render(input_size, buffer);

        // render list
        let index = self.index;
        for (i, cmd) in self.commands().iter().enumerate() {
            let (style, caret) = if i == index {
                (THEME.get("ui.menu.selected"), " ")
            } else {
                (THEME.get("ui.menu"), "  ")
            };
            let y = inner.top() + (2 + i) as u16;
            buffer.put_str(caret, inner.left(), y, style);
            buffer.put_str(cmd.name, inner.left() + 2, y, style);
            buffer.put_str(cmd.desc, inner.right().saturating_sub(cmd.desc.chars().count() as u16), y, style);
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Enter => self.run(ctx),
            KeyCode::Up => {
                self.index = self.index.saturating_sub(1);
                EventResult::Consumed(None)
            }
            KeyCode::Down => {
                self.index = (self.index + 1).min(self.commands().len().saturating_sub(1));
                EventResult::Consumed(None)
            }
            // scroll by a page
            // KeyCode::PageUp => todo!(),
            // KeyCode::PageDown => todo!(),
            KeyCode::Esc => EventResult::Consumed(Some(Box::new(|compositor, _| {
                compositor.pop();
            }))),
            _ => {
                self.input.handle_key_event(event);
                self.index = 0;
                EventResult::Consumed(None)
            }
        }
    }

    fn cursor(&self, _area: Rect, _ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (
            Some(self.input.view.view_cursor_position),
            Some(SetCursorStyle::SteadyBar),
        )
    }
}
