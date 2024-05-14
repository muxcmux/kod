use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}, style::Color};

use crate::{actions, compositor::{Component, Context, EventResult}, document::Document, editor::Mode, keymap::{KeymapResult, Keymaps}, ui::{Buffer, Position, Rect}};

const GUTTER_LINE_NUM_PAD_LEFT: u16 = 2;
const GUTTER_LINE_NUM_PAD_RIGHT: u16 = 1;
const MIN_GUTTER_WIDTH: u16 = 6;

fn adjust_scroll(dimension: usize, doc_cursor: usize, offset: usize, scroll: usize) -> Option<usize> {
    if doc_cursor > dimension.saturating_sub(offset + 1) + scroll {
        return Some(doc_cursor.saturating_sub(dimension.saturating_sub(offset + 1)));
    }

    if doc_cursor < scroll + offset {
        return Some(doc_cursor.saturating_sub(offset));
    }

    None
}

fn gutter_and_document_areas(size: Rect, ctx: &Context) -> (Rect, Rect) {
    let gutter_width = ctx.editor.document.lines_len().checked_ilog10().unwrap_or(1) as u16 + GUTTER_LINE_NUM_PAD_LEFT + GUTTER_LINE_NUM_PAD_RIGHT;
    let gutter_width = gutter_width.max(MIN_GUTTER_WIDTH);
    let gutter_area = size.clip_bottom(2).clip_right(size.width.saturating_sub(gutter_width));
    // clip right to allow for double width graphemes
    let area = size.clip_left(gutter_area.width).clip_right(1);

    (gutter_area, area)
}

#[derive(Debug)]
pub struct EditorView {
    area: Rect,
    gutter_area: Rect,
    cursor_position: Position,
    keymaps: Keymaps,
    offset_x: usize,
    offset_y: usize,
    scroll_x: usize,
    scroll_y: usize,
}

impl EditorView {
    pub fn new(size: Rect, ctx: &Context) -> Self {
        let (gutter_area, area) = gutter_and_document_areas(size, ctx);

        Self {
            area,
            gutter_area,
            cursor_position: area.position,
            keymaps: Keymaps::default(),
            offset_x: 6,
            offset_y: 4,
            scroll_y: 0,
            scroll_x: 0,
        }
    }

    fn row_range(&self) -> std::ops::Range<usize> {
        self.scroll_y..self.scroll_y + self.area.height as usize
    }

    fn col_range(&self) -> std::ops::Range<usize> {
        self.scroll_x..self.scroll_x + self.area.width as usize
    }

    fn ensure_cursor_is_in_view(&mut self, document: &Document) {
        if let Some(s) = adjust_scroll(self.area.height as usize, document.cursor_y, self.offset_y, self.scroll_y) {
            self.scroll_y = s;
        }

        if let Some(s) = adjust_scroll(self.area.width as usize, document.cursor_x, self.offset_x, self.scroll_x) {
            self.scroll_x = s;
        }

        // adjust cursor
        self.cursor_position.y = self.area.top() + document.cursor_y.saturating_sub(self.scroll_y) as u16;
        self.cursor_position.x = self.area.left() + document.cursor_x.saturating_sub(self.scroll_x) as u16;
    }

    fn render_document(&self, buffer: &mut Buffer, ctx: &mut Context) {
        for row in self.row_range() {
            if row >= ctx.editor.document.lines_len() {
                break;
            }
            let line = ctx.editor.document.data.line(row);
            let mut graphemes = line.graphemes();
            let mut skip_next_n_cols = 0;

            // advance the iterator to account for scroll
            let mut advance = 0;
            while advance < self.scroll_x {
                if let Some(g) = graphemes.next() {
                    advance += unicode_display_width::width(&g) as usize;
                    skip_next_n_cols = advance.saturating_sub(self.scroll_x);
                } else {
                    break
                }
            }

            for col in self.col_range() {
                if skip_next_n_cols > 0 {
                    skip_next_n_cols -= 1;
                    continue;
                }
                match graphemes.next() {
                    None => break,
                    Some(g) => {
                        let width = unicode_display_width::width(&g) as usize;
                        let x = col.saturating_sub(self.scroll_x);
                        let y = row.saturating_sub(self.scroll_y);
                        buffer.put_symbol(g.to_string(), x as u16 + self.area.left(), y as u16 + self.area.top(), Color::Reset, Color::Reset);
                        skip_next_n_cols = width - 1;
                    }
                }
            }
        }
    }

    fn render_gutter(&self, buffer: &mut Buffer, ctx: &Context) {
        let max = ctx.editor.document.lines_len();

        for y in self.gutter_area.v_range() {
            let line_no = y as usize + self.scroll_y + 1;
            if line_no > max { break }

            match ctx.editor.mode {
                Mode::Insert => {
                    let label = format!("{: >1$}", line_no, self.gutter_area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize);
                    let fg = if line_no == ctx.editor.document.cursor_y + 1 {
                        Color::White
                    } else {
                        Color::DarkGrey
                    };
                    buffer.put_string(label, 0, y, fg, Color::Reset);
                }
                Mode::Normal => {
                    let rel_line_no = self.cursor_position.y as isize - y as isize;
                    let (fg, label) = if rel_line_no == 0 {
                        (
                            Color::White,
                            format!("  {}", ctx.editor.document.cursor_y + 1)
                        )
                    } else {
                        (
                            Color::DarkGrey,
                            format!("{: >1$}", rel_line_no.abs(), self.gutter_area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize)
                        )
                    };
                    buffer.put_string(label, 0, y, fg, Color::Reset);
                }
            }
        }
    }

    fn handle_keymap_event(&mut self, event: KeyEvent, ctx: &mut actions::Context) -> Option<KeymapResult> {
        let result = self.keymaps.get(&ctx.editor.mode, event.code);

        if let KeymapResult::Found(f) = result {
            f(ctx);
            return None;
        }

        Some(result)
    }

    fn handle_normal_mode_key_event(&mut self, event: KeyEvent, ctx: &mut actions::Context) -> EventResult {
        match self.handle_keymap_event(event, ctx) {
            Some(KeymapResult::NotFound) => EventResult::Ignored(None),
            _ => EventResult::Consumed(None)
        }
    }

    fn handle_insert_mode_key_event(&mut self, event: KeyEvent, ctx: &mut actions::Context) -> EventResult {
        match self.handle_keymap_event(event, ctx) {
            Some(KeymapResult::NotFound) => {
                if let KeyCode::Char(c) = event.code {
                    actions::append_character(c, ctx);
                    EventResult::Consumed(None)
                } else {
                    EventResult::Ignored(None)
                }
            }
            Some(KeymapResult::Cancelled(pending)) => {
                let mut result = EventResult::Ignored(None);
                for key_code in pending {
                    match key_code {
                        KeyCode::Char(c) => {
                            actions::append_character(c, ctx);
                            result = EventResult::Consumed(None);
                        }
                        _ => {
                            if let KeymapResult::Found(f) = self.keymaps.get(&Mode::Insert, key_code) {
                                f(ctx);
                                result = EventResult::Consumed(None)
                            }
                        }
                    }
                }

                result
            }
            _ => EventResult::Consumed(None)
        }
    }
}

impl Component for EditorView {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        self.resize(area.clip_bottom(2), ctx);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
        self.render_document(buffer, ctx);
        self.render_gutter(buffer, ctx);
    }

    fn resize(&mut self, new_size: Rect, ctx: &mut Context) {
        let (gutter_area, area) = gutter_and_document_areas(new_size, ctx);
        self.area = area;
        self.gutter_area = gutter_area;
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        ctx.editor.status = None;

        match ctx.editor.mode {
            Mode::Normal => {
                let mut action_ctx = actions::Context {
                    editor: ctx.editor,
                    compositor_callbacks: vec![]
                };
                self.handle_normal_mode_key_event(event, &mut action_ctx)
            }
            Mode::Insert => {
                let mut action_ctx = actions::Context {
                    editor: ctx.editor,
                    compositor_callbacks: vec![]
                };
                self.handle_insert_mode_key_event(event, &mut action_ctx)
            }
        }
    }

    fn cursor(&self, _area: Rect, ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (
            Some(self.cursor_position),
            Some(
                match ctx.editor.mode {
                    Mode::Normal => SetCursorStyle::SteadyBlock,
                    Mode::Insert => SetCursorStyle::SteadyBar,
                }
            )
        )
    }
}
