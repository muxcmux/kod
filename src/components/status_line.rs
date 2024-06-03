use crate::ui::buffer::Buffer;
use crate::ui::Rect;
use crossterm::style::Color;
use crate::{compositor::{Component, Context}, editor::Mode};

#[derive(Debug)]
pub struct StatusLine {
    area: Rect
}

impl StatusLine {
    pub fn new(area: Rect) -> Self {
        Self { area }
    }
}

impl Component for StatusLine {
    fn resize(&mut self, new_size: Rect, _ctx: &mut Context) {
        self.area = new_size.clip_top(new_size.height.saturating_sub(2));
    }

    fn render(&mut self, _area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let (mut x, y) = (self.area.left(), self.area.top());

        let line = " ".repeat(self.area.width as usize);
        buffer.put_string(line, x, y, Color::White, Color::Black);

        let (label, label_fg, label_bg) = match ctx.editor.mode {
            Mode::Normal => (" NOR ", Color::Black, Color::Blue),
            Mode::Insert => (" INS ", Color::Black, Color::Green),
        };

        buffer.put_string(label.to_string(), x, y, label_fg, label_bg);
        x += (label.chars().count() + 1) as u16;

        let filename = ctx.editor.document.filename();
        let filename_len = filename.chars().count();
        buffer.put_string(filename.into(), x, y, Color::White, Color::Black);
        x += (filename_len + 1) as u16;

        if ctx.editor.document.modified {
            buffer.put_string("*".into(), x, y, Color::Yellow, Color::Black);
            x += 2;
        }

        if ctx.editor.document.readonly {
            buffer.put_string("readonly".into(), x, y, Color::DarkGrey, Color::Black);
        }

        let cursor_position = format!(" {}:{} ", ctx.editor.document.cursor_y + 1, ctx.editor.document.grapheme_at_cursor().0 + 1);
        let w = self.area.width.saturating_sub(cursor_position.chars().count() as u16);
        buffer.put_string(cursor_position, w, y, Color::White, Color::Black);
    }
}

