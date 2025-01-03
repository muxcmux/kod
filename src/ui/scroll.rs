use crate::{selection::Selection, ui::{Position, Rect}};

fn adjust_scroll(dimension: usize, cursor: usize, offset: usize, scroll: usize) -> Option<usize> {
    if cursor > dimension.saturating_sub(offset + 1) + scroll {
        return Some(cursor.saturating_sub(dimension.saturating_sub(offset + 1)));
    }

    if cursor < scroll + offset {
        return Some(cursor.saturating_sub(offset));
    }

    None
}

#[derive(Default, Debug)]
pub struct Scroll {
    // The visual position of a cursor on the screen
    // relative to the origin 0,0 at the top left of
    // the editor (not the current view)
    pub cursor: Position,
    pub offset_x: usize,
    pub offset_y: usize,
    pub x: usize,
    pub y: usize,
}

impl Scroll {
    pub fn ensure_cursor_is_in_view(&mut self, selection: &Selection, area: &Rect) {
        if let Some(s) = adjust_scroll(area.height as usize, selection.head.y, self.offset_y, self.y) {
            self.y = s;
        }

        if let Some(s) = adjust_scroll(area.width as usize, selection.head.x, self.offset_x, self.x) {
            self.x = s;
        }

        // adjust cursor
        self.cursor.row = area.top() + selection.head.y.saturating_sub(self.y) as u16;
        self.cursor.col = area.left() + selection.head.x.saturating_sub(self.x) as u16;
    }
}
