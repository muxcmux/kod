use crate::graphemes;
use crate::ui::style::Style;
use crate::{current, ui::theme::THEME};
use crate::ui::buffer::Buffer;
use crate::ui::Rect;
use crate::compositor::{Component, Context};

#[derive(Debug)]
pub struct StatusLine;

pub fn draw_left(str: impl AsRef<str>, x: u16, y: u16, buffer: &mut Buffer, style: Style) -> u16 {
    buffer.put_str(str.as_ref(), x, y, style);
    x + 1 + graphemes::width(str.as_ref()) as u16
}

pub fn draw_right(str: impl AsRef<str>, right: u16, y: u16, buffer: &mut Buffer, style: Style) -> u16 {
    let width = graphemes::width(str.as_ref());
    let x = right.saturating_sub(width as u16);
    buffer.put_str(str.as_ref(), x, y, style);
    x.saturating_sub(1)
}

pub fn position(area: Rect) -> (u16, u16, Rect) {
    let area = area.clip_top(area.height.saturating_sub(1));
    (area.left(), area.top(), area)
}

pub fn draw_editor_mode(x: u16, y: u16, buffer: &mut Buffer, ctx: &mut Context) -> u16 {
    let (mode, style) = match ctx.editor.mode {
        crate::editor::Mode::Normal => (" NOR ", THEME.get("ui.statusline.normal")),
        crate::editor::Mode::Insert => (" INS ", THEME.get("ui.statusline.insert")),
        crate::editor::Mode::Replace => (" REP ", THEME.get("ui.statusline.replace")),
        crate::editor::Mode::Select => (" SEL ", THEME.get("ui.statusline.select")),
    };

    draw_left(mode, x, y, buffer, style)
}

pub fn draw_cursor_count(right: u16, y: u16, buffer: &mut Buffer, style: Style, ctx: &mut Context) -> u16 {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    if sel.ranges.len() > 1 {
        let cursors = format!("[{} cursors]", sel.ranges.len());
        draw_right(&cursors, right, y, buffer, style)
    } else {
        right
    }
}

pub fn draw_search_matches(right: u16, y: u16, buffer: &mut Buffer, style: Style, ctx: &mut Context) -> u16 {
    if ctx.editor.search.total_matches > 0 {
        let label = format!("{}/{}", ctx.editor.search.current_match + 1, ctx.editor.search.total_matches);
        draw_right(&label, right, y, buffer, style)
    } else {
        right
    }
}

pub fn draw_background(area: Rect, buffer: &mut Buffer)  {
    buffer.set_style(area, THEME.get("ui.statusline"));
}

impl Component for StatusLine {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let (mut x, y, area) = position(area);

        draw_background(area, buffer);
        x = draw_editor_mode(x, y, buffer, ctx);

        let (_, doc) = current!(ctx.editor);
        match &ctx.editor.status {
            Some(status) => {
                let style = THEME.get(match status.severity {
                    crate::editor::Severity::Hint => "hint",
                    crate::editor::Severity::Info => "info",
                    crate::editor::Severity::Warning => "warning",
                    crate::editor::Severity::Error => "error",
                });

                _ = draw_left(&status.message, x, y, buffer, style);
            },

            None => {
                if let Some(lang) = &doc.language {
                    if let Some(ref icon) = lang.icon {
                        x = draw_left(icon, x, y, buffer, THEME.get("ui.statusline.filename"));
                    }
                }

                x = draw_left(doc.filename_display(), x, y, buffer, THEME.get("ui.statusline.filename"));

                if doc.is_modified() {
                    x = draw_left("[+]", x, y, buffer, THEME.get("ui.statusline.modified"));
                }

                if doc.readonly {
                    _ = draw_left("[readonly]", x, y, buffer, THEME.get("ui.statusline.read_only"));
                }
            },
        }

        _ = draw_cursor_count(area.right().saturating_sub(1), y, buffer,THEME.get("ui.statusline.cursor_len"), ctx);
    }
}

