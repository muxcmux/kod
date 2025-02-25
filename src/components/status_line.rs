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
        buffer.set_style(area, THEME.get("ui.statusline"));

        // editor mode
        let (mode, style) = match ctx.editor.mode {
            crate::editor::Mode::Normal => (" NOR ", THEME.get("ui.statusline.normal")),
            crate::editor::Mode::Insert => (" INS ", THEME.get("ui.statusline.insert")),
            crate::editor::Mode::Replace => (" REP ", THEME.get("ui.statusline.replace")),
            crate::editor::Mode::Select => (" SEL ", THEME.get("ui.statusline.select")),
        };

        buffer.put_str(mode, x, y, style);

        x += 6_u16;

        let (pane, doc) = current!(ctx.editor);
        match &ctx.editor.status {
            Some(status) => {
                let style = THEME.get(match status.severity {
                    crate::editor::Severity::Hint => "hint",
                    crate::editor::Severity::Info => "info",
                    crate::editor::Severity::Warning => "warning",
                    crate::editor::Severity::Error => "error",
                });

                buffer.put_symbol("●", x, y, style);
                buffer.put_str(&status.message, x + 2, y, style);
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

                if doc.is_modified() {
                    buffer.put_str("[+]", x, y, THEME.get("ui.statusline.modified"));
                    x += 4;
                }

                if doc.readonly {
                    buffer.put_str("[readonly]", x, y, THEME.get("ui.statusline.read_only"));
                }
            },
        }

        let sel = doc.selection(pane.id);
        let cursor_position = format!("{}:{} ", sel.primary().head.y + 1, sel.primary().grapheme_at_head(&doc.rope).0 + 1);
        let cursor_position_len = cursor_position.chars().count() as u16;
        let w = area.width.saturating_sub(cursor_position_len);
        buffer.put_str(&cursor_position, w, y, THEME.get("ui.statusline.cursor_pos"));

        if sel.ranges.len() > 1 {
            let cursors = format!("󰆿({}) ", sel.ranges.len());
            let x = area.width.saturating_sub(cursor_position_len + cursors.chars().count() as u16);
            buffer.put_str(&cursors, x, y, THEME.get("ui.statusline.cursor_len"));
        }
    }
}

