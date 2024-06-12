use crate::commands;
use crate::compositor;
use crate::ui::buffer::Buffer;
use crate::ui::Position;
use crate::ui::Rect;
use crossterm::{
    cursor::SetCursorStyle,
    event::{KeyCode, KeyEvent},
    style::Color,
};

use crate::{
    commands::{actions, KeyCallback},
    compositor::{Component, Context, EventResult},
    editor::Mode,
    keymap::{KeymapResult, Keymaps},
};

use super::scroll_view::ScrollView;

const GUTTER_LINE_NUM_PAD_LEFT: u16 = 2;
const GUTTER_LINE_NUM_PAD_RIGHT: u16 = 1;
const MIN_GUTTER_WIDTH: u16 = 6;

fn gutter_and_document_areas(size: Rect, ctx: &Context) -> (Rect, Rect) {
    let gutter_width = ctx
        .editor
        .document
        .text
        .lines_len()
        .checked_ilog10()
        .unwrap_or(1) as u16
        + GUTTER_LINE_NUM_PAD_LEFT
        + GUTTER_LINE_NUM_PAD_RIGHT;
    let gutter_width = gutter_width.max(MIN_GUTTER_WIDTH);
    let gutter_area = size
        .clip_bottom(1)
        .clip_right(size.width.saturating_sub(gutter_width));
    // clip right to allow for double width graphemes
    let area = size.clip_left(gutter_area.width).clip_right(1);

    (gutter_area, area)
}

fn compute_offset(size: Rect) -> (usize, usize) {
    (
        ((size.width as usize).saturating_sub(1).max(1) / 2).min(6),
        ((size.height as usize).saturating_sub(1).max(1) / 2).min(4),
    )
}

#[derive(Default)]
pub struct EditorView {
    scroll_view: ScrollView,
    keymaps: Keymaps,
    on_next_key: Option<KeyCallback>,
}

impl EditorView {
    fn render_document(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        self.scroll_view.render(
            area,
            buffer,
            &ctx.editor.document.text,
            |buf: &mut Buffer, (x, y)| {
                // render trailing whitespace
                buf.put_symbol("~", x, y, Color::DarkGrey, Color::Reset);
            },
        );
    }

    fn render_gutter(&self, area: Rect, buffer: &mut Buffer, ctx: &Context) {
        let max = ctx.editor.document.text.lines_len();

        for y in area.top()..=area.bottom() {
            let line_no = y as usize + self.scroll_view.scroll_y + 1;
            if line_no > max {
                break;
            }

            match ctx.editor.mode {
                Mode::Insert => {
                    let label = format!(
                        "{: >1$}",
                        line_no,
                        area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
                    );
                    let fg = if line_no == ctx.editor.document.text.cursor_y + 1 {
                        Color::White
                    } else {
                        Color::DarkGrey
                    };
                    buffer.put_str(&label, 0, y, fg, Color::Reset);
                }
                Mode::Normal => {
                    let rel_line_no = self.scroll_view.cursor_position.y as isize - y as isize;
                    let (fg, label) = if rel_line_no == 0 {
                        (
                            Color::White,
                            format!("  {}", ctx.editor.document.text.cursor_y + 1),
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
                    buffer.put_str(&label, 0, y, fg, Color::Reset);
                }
            }
        }
    }

    fn handle_keymap_event(
        &mut self,
        event: KeyEvent,
        ctx: &mut commands::Context,
    ) -> Option<KeymapResult> {
        let result = self.keymaps.get(&ctx.editor.mode, event);

        if let KeymapResult::Found(f) = result {
            f(ctx);
            return None;
        }

        Some(result)
    }

    fn handle_normal_mode_key_event(
        &mut self,
        event: KeyEvent,
        ctx: &mut commands::Context,
    ) -> EventResult {
        match self.handle_keymap_event(event, ctx) {
            Some(KeymapResult::NotFound) => EventResult::Ignored(None),
            _ => EventResult::Consumed(None),
        }
    }

    fn handle_insert_mode_key_event(
        &mut self,
        event: KeyEvent,
        ctx: &mut commands::Context,
    ) -> EventResult {
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
                for event in pending {
                    match event.code {
                        KeyCode::Char(c) => {
                            actions::append_character(c, ctx);
                            result = EventResult::Consumed(None);
                        }
                        _ => {
                            if let KeymapResult::Found(f) = self.keymaps.get(&Mode::Insert, event) {
                                f(ctx);
                                result = EventResult::Consumed(None)
                            }
                        }
                    }
                }

                result
            }
            _ => EventResult::Consumed(None),
        }
    }
}

impl Component for EditorView {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let (gutter_area, editor_area) = gutter_and_document_areas(area.clip_bottom(1), ctx);
        (self.scroll_view.offset_x, self.scroll_view.offset_y) = compute_offset(editor_area);
        self.render_document(editor_area, buffer, ctx);
        self.render_gutter(gutter_area, buffer, ctx);
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        ctx.editor.status = None;

        let mut action_ctx = commands::Context {
            editor: ctx.editor,
            compositor_callbacks: vec![],
            on_next_key_callback: None,
        };

        let event_result = if let Some(on_next_key) = self.on_next_key.take() {
            on_next_key(&mut action_ctx, event);
            EventResult::Consumed(None)
        } else {
            match action_ctx.editor.mode {
                Mode::Normal => self.handle_normal_mode_key_event(event, &mut action_ctx),
                Mode::Insert => self.handle_insert_mode_key_event(event, &mut action_ctx),
            }
        };

        self.on_next_key = action_ctx.on_next_key_callback.take();

        let callback = if action_ctx.compositor_callbacks.is_empty() {
            None
        } else {
            let cb: compositor::Callback = Box::new(|compositor, cx| {
                for cb in action_ctx.compositor_callbacks {
                    cb(compositor, cx);
                }
            });

            Some(cb)
        };

        match event_result {
            EventResult::Ignored(_) => EventResult::Ignored(callback),
            EventResult::Consumed(_) => EventResult::Consumed(callback),
        }
    }

    fn handle_paste(&mut self, str: &str, ctx: &mut Context) -> EventResult {
        ctx.editor.document.text.insert_str_at_cursor(str, &ctx.editor.mode);
        ctx.editor.document.modified = true;
        EventResult::Consumed(None)
    }

    fn cursor(&self, _area: Rect, ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (
            Some(self.scroll_view.cursor_position),
            Some(match ctx.editor.mode {
                Mode::Normal => SetCursorStyle::SteadyBlock,
                Mode::Insert => SetCursorStyle::SteadyBar,
            }),
        )
    }
}
