use crate::ui::buffer::Buffer;
use crate::ui::Rect;
use crossterm::style::Color;
use crate::{compositor::{Component, Context}, editor::Mode};

#[derive(Debug)]
pub struct StatusLine;

impl Component for StatusLine {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let area = area.clip_top(area.height.saturating_sub(1));

        let (mut x, y) = (area.left(), area.top());

        let line = " ".repeat(area.width as usize);
        buffer.put_str(&line, x, y, Color::White, Color::Black);

        let (label, label_fg, label_bg) = match ctx.editor.mode {
            Mode::Normal => (" NOR ", Color::Black, Color::Blue),
            Mode::Insert => (" INS ", Color::Black, Color::Green),
        };

        buffer.put_str(label, x, y, label_fg, label_bg);
        x += (label.chars().count() + 1) as u16;

        let filename = ctx.editor.document.filename();
        let filename_len = filename.chars().count();
        buffer.put_str(&filename, x, y, Color::White, Color::Black);
        x += (filename_len + 1) as u16;

        if ctx.editor.document.modified {
            buffer.put_str("*", x, y, Color::Yellow, Color::Black);
            x += 2;
        }

        if ctx.editor.document.readonly {
            buffer.put_str("readonly", x, y, Color::DarkGrey, Color::Black);
        }

        let cursor_position = format!(" {}:{} ", ctx.editor.document.text.cursor_y + 1, ctx.editor.document.text.grapheme_at_cursor().0 + 1);
        let w = area.width.saturating_sub(cursor_position.chars().count() as u16);
        buffer.put_str(&cursor_position, w, y, Color::White, Color::Black);
    }
}

