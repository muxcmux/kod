use crate::{document::Document, editor::Mode, selection::Selection, ui::{buffer::Buffer, theme::THEME, Rect}, view::View};

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


pub fn render(
    view: &View,
    sel: &Selection,
    area: Rect,
    buffer: &mut Buffer,
    doc: &Document,
    mode: &Mode,
    active: bool
) {
    let max = doc.rope.line_len();

    for y in 0..=area.height {
        let line_no = y as usize + view.scroll.y + 1;

        if line_no > max {
            break;
        }

        if active {
            match mode {
                Mode::Insert | Mode::Replace =>
                    absolute(line_no, y + area.top(), area, buffer, sel),
                _ =>
                    relative(y + area.top(), area, buffer, view, sel)
            }
        } else {
            absolute(line_no, y + area.top(), area, buffer, sel);
        }
    }
}

fn absolute(line_no: usize, y: u16, area: Rect, buffer: &mut Buffer, sel: &Selection) {
    let label = format!(
        "{: >1$}",
        line_no,
        area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
    );
    let style = if line_no == sel.head.y + 1 {
        "ui.linenr.selected"
    } else {
        "ui.linenr"
    };
    buffer.put_str(&label, area.left(), y, THEME.get(style));
}

fn relative(y: u16, area: Rect, buffer: &mut Buffer, view: &View, sel: &Selection) {
    let rel_line_no = view.scroll.cursor.row as isize - y as isize;
    let (style, label) = if rel_line_no == 0 {
        (
            "ui.linenr.selected",
            format!("  {}", sel.head.y + 1),
        )
    } else {
        (
            "ui.linenr",
            format!(
                "{: >1$}",
                rel_line_no.abs(),
                area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
            ),
        )
    };
    let style = THEME.get(style);
    buffer.put_str(&label, area.left(), y, style);
}
