use crate::{
    components::scroll_view::ScrollView, compositor::{Component, Context, EventResult}, editor::Mode, graphemes::NEW_LINE, ui::{
        border_box::BorderBox,
        borders::{Stroke, Borders},
        buffer::Buffer,
        Position, Rect,
    }
};
use crop::Rope;
use crossterm::{
    cursor::SetCursorStyle,
    event::{KeyCode, KeyEvent},
    style::Color,
};

use super::{Command, COMMANDS};

pub struct Pallette {
    input: TextInput,
    index: usize,
}

impl Pallette {
    pub fn new() -> Self {
        Self {
            input: TextInput::new(Rope::from("\n")),
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
        let text = self.input.rope.to_string();
        let text = text.trim();
        COMMANDS
            .iter()
            .filter(|c| {
                text == "\n" || c.name.contains(text) || c.aliases.iter().any(|c| *c == text)
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
            .stroke(Stroke::Rounded);

        bbox.render(buffer).split_horizontally(2, buffer);

        let inner = bbox.inner();
        let input_size = inner.clip_bottom(inner.height.saturating_sub(1));

        self.input.render(input_size, buffer);

        // render list
        let index = self.index;
        for (i, cmd) in self.commands().iter().enumerate() {
            let (fg, caret) = if i == index {
                (Color::White, " ")
            } else {
                (Color::DarkGrey, "  ")
            };
            let y = inner.top() + (2 + i) as u16;
            buffer.put_str(caret, inner.left(), y, fg, Color::Reset);
            buffer.put_str(cmd.name, inner.left() + 2, y, fg, Color::Reset);
            buffer.put_str(cmd.desc, inner.right().saturating_sub(cmd.desc.chars().count() as u16), y, fg, Color::Reset);
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

struct TextInput {
    view: ScrollView,
    rope: Rope,
}

impl TextInput {
    fn new(rope: Rope) -> Self {
        Self {
            rope,
            view: ScrollView::default(),
        }
    }

    fn render(&mut self, area: Rect, buffer: &mut Buffer) {
        self.view.render(area, buffer, &self.rope, |_, _| {});
    }

    fn insert_char_at_cursor(&mut self, char: char, mode: &Mode) {
        let offset = self.view.byte_offset_at_cursor(&mut self.rope, self.view.text_cursor_x, self.view.text_cursor_y);
        let mut buf = [0; 4];
        let text = char.encode_utf8(&mut buf);

        self.rope.insert(offset, text);

        if char == NEW_LINE {
            self.view.move_cursor_to(&mut self.rope, Some(0), Some(self.view.text_cursor_y + 1), mode);
        } else {
            self.view.move_cursor_to(&mut self.rope, Some(self.view.text_cursor_x + 1), None, mode);
        }
    }

    pub fn delete_to_the_left(&mut self, mode: &Mode) -> bool {
        if self.view.text_cursor_x > 0 {
            let mut start = self.rope.byte_of_line(self.view.text_cursor_y);
            let mut end = start;
            let idx = self.view.grapheme_at_cursor(&self.rope).0 - 1;
            for (i, g) in self.rope.line(self.view.text_cursor_y).graphemes().enumerate() {
                if i < idx { start += g.len() }
                if i == idx {
                    end = start + g.len();
                    break
                }
            }

            self.view.cursor_left(&self.rope, &Mode::Insert);
            self.rope.delete(start..end);
            return true;
        } else if self.view.text_cursor_y > 0  {
            let to = self.rope.byte_of_line(self.view.text_cursor_y);
            let from = to.saturating_sub(NEW_LINE.len_utf8());
            // need to move cursor before deleting
            self.view.move_cursor_to(&self.rope, Some(self.view.line_width(&self.rope, self.view.text_cursor_y - 1)), Some(self.view.text_cursor_y - 1), mode);
            self.rope.delete(from..to);
            return true;
        }

        false
    }

    fn handle_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Left => {
                self.view.cursor_left(&self.rope, &Mode::Insert);
            }
            KeyCode::Right => {
                self.view.cursor_right(&self.rope, &Mode::Insert);
            }
            KeyCode::Up => {
                self.view.cursor_up(&self.rope, &Mode::Insert);
            }
            KeyCode::Down => {
                self.view.cursor_down(&self.rope, &Mode::Insert);
            }
            KeyCode::Home => {
                self.view.move_cursor_to(&self.rope, Some(0), None, &Mode::Insert);
            }
            KeyCode::End => {
                self.view.move_cursor_to(&self.rope, Some(usize::MAX), None, &Mode::Insert);
            }
            KeyCode::Backspace => {
                self.delete_to_the_left(&Mode::Insert);
            }
            KeyCode::Char(c) => {
                self.insert_char_at_cursor(c, &Mode::Insert);
            }
            _ => {}
        }
    }
}
