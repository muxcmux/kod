use crossterm::style::Color;
use crate::{compositor::{Component, Context}, editor::Mode, ui::{Buffer, Rect}};

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
        self.area = new_size.clip_top(new_size.height.saturating_sub(1));
    }

    fn render(&mut self, _area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let (x, y) = (self.area.left(), self.area.top());

        let line = " ".repeat(self.area.width as usize);
        buffer.put_string(line, x, y, Color::White, Color::Black);

        let (label, label_fg, label_bg) = match ctx.editor.mode {
            Mode::Normal => (" NOR ", Color::Black, Color::Blue),
            Mode::Insert => (" INS ", Color::Black, Color::Green),
        };

        buffer.put_string(label.to_string(), x, y, label_fg, label_bg);

        let filename = match &ctx.editor.document.path {
            Some(p) => p.to_str().expect("shit path name given"),
            None => "[scratch]",
        };
        buffer.put_string(filename.to_string(), label.chars().count() as u16 + 1, y, Color::White, Color::Black);

        if ctx.editor.document.modified {
            let x = filename.chars().count() + label.chars().count() + 2;
            buffer.put_string("[*]".to_string(), x as u16, y, Color::White, Color::Black);
        }

        let cursor_position = format!(" {}:{} ", ctx.editor.document.cursor_y + 1, ctx.editor.document.grapheme_idx_at_cursor() + 1);
        let w = self.area.width.saturating_sub(cursor_position.chars().count() as u16);
        buffer.put_string(cursor_position, w, y, Color::White, Color::Black);
    }
}

