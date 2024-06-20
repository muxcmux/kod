use crate::ui::buffer::Buffer;
use crate::ui::Rect;
use crossterm::style::Color;
use crate::{compositor::{Component, Context}, editor::Mode};

#[derive(Debug)]
pub struct StatusLine;

impl Component for StatusLine {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let area = area.clip_top(area.height.saturating_sub(1));
        let bg = Color::Black;

        let (mut x, y) = (area.left(), area.top());

        // draw background
        let line = " ".repeat(area.width as usize);
        buffer.put_str(&line, x, y, Color::White, bg);

        //// draw mode
        //let (label, label_fg, label_bg) = match ctx.editor.mode {
        //    Mode::Normal => (" NOR ", bg, Color::Blue),
        //    Mode::Insert => (" INS ", bg, Color::Green),
        //};
        //
        //buffer.put_str(label, x, y, label_fg, label_bg);
        //x += (label.chars().count() + 1) as u16;
        x += 1_u16;

        match &ctx.editor.status {
            Some(status) => {
                let fg = match status.severity {
                    crate::editor::Severity::Hint => Color::Blue,
                    crate::editor::Severity::Info => Color::White,
                    crate::editor::Severity::Warning => Color::Yellow,
                    crate::editor::Severity::Error => Color::Red,
                };

                buffer.put_str(&status.message, x, y, fg, bg);
            },

            None => {
                let filename = ctx.editor.document.filename();
                let filename_len = filename.chars().count();
                buffer.put_str(&filename, x, y, Color::White, bg);
                x += (filename_len + 1) as u16;

                if ctx.editor.document.modified {
                    buffer.put_str("[+]", x, y, Color::Yellow, bg);
                    x += 4;
                }

                if ctx.editor.document.readonly {
                    buffer.put_str("[readonly]", x, y, Color::DarkGrey, bg);
                }
            },
        }

        let cursor_position = format!(" {}:{} ", ctx.editor.document.text.cursor_y + 1, ctx.editor.document.text.grapheme_at_cursor().0 + 1);
        let w = area.width.saturating_sub(cursor_position.chars().count() as u16);
        buffer.put_str(&cursor_position, w, y, Color::White, bg);
    }
}

