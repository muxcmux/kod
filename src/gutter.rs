use crossterm::style::Color;

use crate::{components::scroll_view::ScrollView, document::Document, editor::Mode, ui::{buffer::Buffer, Rect}};

const GUTTER_LINE_NUM_PAD_LEFT: u16 = 2;
const GUTTER_LINE_NUM_PAD_RIGHT: u16 = 1;
const MIN_GUTTER_WIDTH: u16 = 6;

pub fn gutter_and_document_areas(size: Rect, doc: &Document) -> (Rect, Rect) {
    let gutter_width = doc
        .rope
        .line_len()
        .checked_ilog10()
        .unwrap_or(1) as u16
        + 1
        + GUTTER_LINE_NUM_PAD_LEFT
        + GUTTER_LINE_NUM_PAD_RIGHT;
    let gutter_width = gutter_width.max(MIN_GUTTER_WIDTH);

    // why do we clip bottom here?
    let gutter_area = size
        .clip_bottom(1)
        .clip_right(size.width.saturating_sub(gutter_width));

    let area = size.clip_left(gutter_area.width);

    (gutter_area, area)
}

pub fn compute_offset(size: Rect) -> (usize, usize) {
    (
        ((size.width as usize).saturating_sub(1).max(1) / 2).min(6),
        ((size.height as usize).saturating_sub(1).max(1) / 2).min(4),
    )
}


pub fn render(view: &ScrollView, area: Rect, buffer: &mut Buffer, doc: &Document, mode: &Mode, active: bool) {
    fn absolute(line_no: usize, y: u16, area: Rect, buffer: &mut Buffer, view: &ScrollView) {
        let label = format!(
            "{: >1$}",
            line_no,
            area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
        );
        let fg = if line_no == view.text_cursor_y + 1 {
            Color::White
        } else {
            Color::DarkGrey
        };
        buffer.put_str(&label, area.left(), y, fg, Color::Reset);
    }

    fn relative(y: u16, area: Rect, buffer: &mut Buffer, view: &ScrollView) {
        let rel_line_no = view.view_cursor_position.y as isize - y as isize;
        let (fg, label) = if rel_line_no == 0 {
            (
                Color::White,
                format!("  {}", view.text_cursor_y + 1),
            )
        } else {
            (
                Color::DarkGrey,
                format!(
                    "{: >1$}",
                    rel_line_no.abs(),
                    area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
                ),
            )
        };
        buffer.put_str(&label, area.left(), y, fg, Color::Reset);
    }

    let max = doc.rope.line_len();

    for y in 0..=area.height {
        let line_no = y as usize + view.scroll_y + 1;

        if line_no > max {
            break;
        }

        if active {
            match mode {
                Mode::Insert | Mode::Replace =>
                    absolute(line_no, y + area.top(), area, buffer, view),
                Mode::Normal =>
                    relative(y + area.top(), area, buffer, view)
            }
        } else {
            absolute(line_no, y + area.top(), area, buffer, view);
        }
    }
}
