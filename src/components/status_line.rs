use crate::{current, ui::theme::THEME};
use crate::ui::buffer::Buffer;
use crate::ui::Rect;
use crate::compositor::{Component, Context};

#[derive(Debug)]
pub struct StatusLine;

impl Component for StatusLine {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let area = area.clip_top(area.height.saturating_sub(1));

        let (mut x, y) = (area.left(), area.top());

        // draw background
        let line = " ".repeat(area.width as usize);
        buffer.put_str(&line, x, y, THEME.get("ui.statusline"));

        x += 1_u16;

        let (pane, doc) = current!(ctx.editor);
        match &ctx.editor.status {
            Some(status) => {
                let style = match status.severity {
                    crate::editor::Severity::Hint => "hint",
                    crate::editor::Severity::Info => "info",
                    crate::editor::Severity::Warning => "warning",
                    crate::editor::Severity::Error => "error",
                };

                buffer.put_str(&status.message, x, y, THEME.get(style));
            },

            None => {
                if let Some(lang) = &doc.language {
                    if let Some(ref icon) = lang.icon {
                        buffer.put_str(icon, x, y, THEME.get("ui.statusline.filename"));
                        x += 2;
                    }
                }

                let filename = doc.filename_display();
                let filename_len = filename.chars().count();
                buffer.put_str(&filename, x, y, THEME.get("ui.statusline.filename"));
                x += (filename_len + 1) as u16;

                if doc.modified {
                    buffer.put_str("[+]", x, y, THEME.get("ui.statusline.modified"));
                    x += 4;
                }

                if doc.readonly {
                    buffer.put_str("[readonly]", x, y, THEME.get("ui.statusline.read_only"));
                }
            },
        }

        let sel = doc.selection(pane.id);
        let cursor_position = format!(" {}:{} ", sel.head.y + 1, sel.grapheme_at_head(&doc.rope).0 + 1);
        let w = area.width.saturating_sub(cursor_position.chars().count() as u16);
        buffer.put_str(&cursor_position, w, y, THEME.get("ui.statusline.cursor_pos"));
    }
}

