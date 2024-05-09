use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}, style::Color};

use crate::{compositor::{Component, Context, EventResult}, document::{Document, HorizontalMove, VerticalMove}, editor::Mode, ui::{Buffer, Position, Rect}};

const GUTTER_LINE_NUM_PAD_LEFT: u16 = 2;
const GUTTER_LINE_NUM_PAD_RIGHT: u16 = 1;
const MIN_GUTTER_WIDTH: u16 = 6;

fn gutter_and_document_areas(size: Rect, ctx: &Context) -> (Rect, Rect) {
    let gutter_width = ctx.editor.document.lines_len().checked_ilog10().unwrap_or(1) as u16 + GUTTER_LINE_NUM_PAD_LEFT + GUTTER_LINE_NUM_PAD_RIGHT;
    let gutter_width = gutter_width.max(MIN_GUTTER_WIDTH);
    let gutter_area = size.clip_bottom(1).clip_right(size.width.saturating_sub(gutter_width));
    // clip right to allow for double width graphemes
    let area = size.clip_left(gutter_area.width).clip_right(1);

    (gutter_area, area)
}

pub struct EditorView {
    area: Rect,
    gutter_area: Rect,
    cursor_position: Position,
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
            offset_x: 10,
            offset_y: 2,
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
        if let Some(ref dir) = document.last_vertical_move_dir {
            match dir {
                VerticalMove::Up => self.scroll_up(document),
                VerticalMove::Down => self.scroll_down(document),
            }
        }

        if let Some(ref dir) = document.last_horizontal_move_dir {
            match dir {
                HorizontalMove::Left => self.scroll_left(document),
                HorizontalMove::Right => self.scroll_right(document),
            }
        }

        // adjust cursor
        self.cursor_position.y = self.area.top() + document.cursor_y.saturating_sub(self.scroll_y) as u16;
        self.cursor_position.x = self.area.left() + document.cursor_x.saturating_sub(self.scroll_x) as u16;
    }

    fn scroll_up(&mut self, document: &Document) {
        self.scroll_y = document.cursor_y.saturating_sub(self.offset_y).min(self.scroll_y);
    }

    fn scroll_down(&mut self, document: &Document) {
        let max_scroll_y = document.lines_len().saturating_sub(self.area.height as usize);
        let scroll_y = document.cursor_y.saturating_sub((self.area.height as usize).saturating_sub(self.offset_y + 1)).min(max_scroll_y);
        self.scroll_y = self.scroll_y.max(scroll_y);
    }

    fn scroll_left(&mut self, document: &Document) {
        self.scroll_x = document.cursor_x.saturating_sub(self.offset_x).min(self.scroll_x);
    }

    fn scroll_right(&mut self, document: &Document) {
        let max_scroll_x = document.line_len(document.cursor_y).saturating_sub(self.area.width as usize);
        let scroll_x = document.cursor_x.saturating_sub((self.area.width as usize).saturating_sub(self.offset_x + 1)).min(max_scroll_x);
        self.scroll_x = self.scroll_x.max(scroll_x);
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

    fn enter_normal_mode(&mut self, ctx: &mut Context) {
        ctx.editor.mode = Mode::Normal;
        ctx.editor.document.cursor_left(&ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn enter_insert_mode_relative_to_cursor(&mut self, x: usize, ctx: &mut Context) {
        ctx.editor.mode = Mode::Insert;
        for _ in 0..x {
            ctx.editor.document.cursor_right(&ctx.editor.mode);
        }
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn enter_insert_mode_at_eol(&mut self, ctx: &mut Context) {
        ctx.editor.mode = Mode::Insert;
        ctx.editor.document.move_cursor_to(Some(ctx.editor.document.current_line_len()), None, &ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn append_character(&mut self, c: char, ctx: &mut Context) {
        ctx.editor.document.insert_char_at_cursor(c, &ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn cursor_up(&mut self, ctx: &mut Context) {
        ctx.editor.document.cursor_up(&ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn cursor_down(&mut self, ctx: &mut Context) {
        ctx.editor.document.cursor_down(&ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn cursor_left(&mut self, ctx: &mut Context) {
        ctx.editor.document.cursor_left(&ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn cursor_right(&mut self, ctx: &mut Context) {
        ctx.editor.document.cursor_right(&ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn go_to_first_line(&mut self, ctx: &mut Context) {
        ctx.editor.document.move_cursor_to(None, Some(0), &ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn go_to_last_line(&mut self, ctx: &mut Context) {
        ctx.editor.document.move_cursor_to(None, Some(ctx.editor.document.lines_len() - 1), &ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn insert_line_below(&mut self, ctx: &mut Context) {
        ctx.editor.mode = Mode::Insert;
        ctx.editor.document.move_cursor_to(Some(std::usize::MAX), None, &ctx.editor.mode);
        ctx.editor.document.insert_char_at_cursor('\n', &ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn insert_line_above(&mut self, ctx: &mut Context) {
        ctx.editor.mode = Mode::Insert;
        ctx.editor.document.move_cursor_to(Some(std::usize::MAX), Some(ctx.editor.document.cursor_y.saturating_sub(1)), &ctx.editor.mode);
        ctx.editor.document.insert_char_at_cursor('\n', &ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn delete_symbol_to_the_left(&mut self, ctx: &mut Context) {
        ctx.editor.document.delete_to_the_left(&ctx.editor.mode);
        self.ensure_cursor_is_in_view(&ctx.editor.document);
    }

    fn save(&self, ctx: &mut Context) {
        ctx.editor.save_document();
    }

    fn quit(&self, ctx: &mut Context) {
        ctx.editor.quit = true;
    }

    fn handle_normal_mode_key_event(&mut self, event: &KeyEvent, ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.cursor_left(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('j') | KeyCode::Down=> {
                self.cursor_down(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cursor_up(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.cursor_right(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('i')=> {
                self.enter_insert_mode_relative_to_cursor(0, ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('a') => {
                self.enter_insert_mode_relative_to_cursor(1, ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('A') => {
                self.enter_insert_mode_at_eol(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('o') => {
                self.insert_line_below(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('O') => {
                self.insert_line_above(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('g') => {
                self.go_to_first_line(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('G') => {
                self.go_to_last_line(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('q') => {
                self.quit(ctx);
                EventResult::Consumed(None)
            }
            KeyCode::Char('s') => {
                self.save(ctx);
                EventResult::Consumed(None)
            }
            _ => EventResult::Ignored(None),
        }
    }

    fn handle_insert_mode_key_event(&mut self, event: &KeyEvent, ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Esc => {
                self.enter_normal_mode(ctx);
                EventResult::Consumed(None)
            },
            KeyCode::Char(c) => {
                self.append_character(c, ctx);
                EventResult::Consumed(None)
            },
            KeyCode::Enter => {
                self.append_character('\n', ctx);
                EventResult::Consumed(None)
            },
            KeyCode::Backspace => {
                self.delete_symbol_to_the_left(ctx);
                EventResult::Consumed(None)
            },
            KeyCode::Left => {
                self.cursor_left(ctx);
                EventResult::Consumed(None)
            },
            KeyCode::Down=> {
                self.cursor_down(ctx);
                EventResult::Consumed(None)
            },
            KeyCode::Up => {
                self.cursor_up(ctx);
                EventResult::Consumed(None)
            },
            KeyCode::Right => {
                self.cursor_right(ctx);
                EventResult::Consumed(None)
            },
            _ => EventResult::Ignored(None)
        }
    }
}

impl Component for EditorView {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        self.resize(area.clip_bottom(1), ctx);
        self.render_document(buffer, ctx);
        self.render_gutter(buffer, ctx);
    }

    fn resize(&mut self, new_size: Rect, ctx: &mut Context) {
        let (gutter_area, area) = gutter_and_document_areas(new_size, ctx);
        self.area = area;
        self.gutter_area = gutter_area;
    }

    fn handle_key_event(&mut self, event: &KeyEvent, ctx: &mut Context) -> EventResult {
        match ctx.editor.mode {
            Mode::Normal => self.handle_normal_mode_key_event(event, ctx),
            Mode::Insert => self.handle_insert_mode_key_event(event, ctx),
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
