use crate::ui::{Position, Rect};

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
    // Adjusts the x and y so that the cursor is always visible
    // if max_y is Some, then it does not leave empty spaces at the bottom
    pub fn ensure_point_is_visible(&mut self, x: usize, y: usize, area: &Rect, max_y: Option<usize>) {
        let offset_y = max_y.map(|len| len.saturating_sub(y + 1).min(self.offset_y)).unwrap_or(self.offset_y);
        if let Some(s) = adjust_scroll(area.height as usize, y, offset_y, self.y) {
            self.y = s;
        }

        // could do the same for offset_x, which will require a max_x as well
        if let Some(s) = adjust_scroll(area.width as usize, x, self.offset_x, self.x) {
            self.x = s;
        }

        // adjust cursor
        self.cursor.row = area.top() + y.saturating_sub(self.y) as u16;
        self.cursor.col = area.left() + x.saturating_sub(self.x) as u16;
    }

    // Adjusts the offsets based on an area
    // Usually called before ensure_point_is_visible
    pub fn adjust_offset(&mut self, area: &Rect, max_x: usize, max_y: usize) {
        self.offset_x = ((area.width as usize).saturating_sub(1).max(1) / 2).min(max_x);
        self.offset_y = ((area.height as usize).saturating_sub(1).max(1) / 2).min(max_y);
    }
}
