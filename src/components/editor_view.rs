use std::collections::HashMap;

use crate::commands;
use crate::compositor;
use crate::current;
use crate::gutter;
use crate::pane;
use crate::panes::PaneId;
use crate::ui::buffer::Buffer;
use crate::ui::Position;
use crate::ui::Rect;
use crossterm::{
    cursor::SetCursorStyle,
    event::{KeyCode, KeyEvent},
};

use crate::{
    commands::{actions, KeyCallback},
    compositor::{Component, Context, EventResult},
    editor::Mode,
    keymap::{KeymapResult, Keymaps},
};

#[derive(Default)]
pub struct EditorView {
    keymaps: Keymaps,
    on_next_key: Option<KeyCallback>,
    waiting_for_input: bool,
}

impl EditorView {
    fn handle_keymap_event(
        &mut self,
        event: KeyEvent,
        ctx: &mut commands::Context,
    ) -> Option<KeymapResult> {
        let result = self.keymaps.get(&ctx.editor.mode, event);

        self.waiting_for_input = matches!(result, KeymapResult::Pending);

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
        char_func: fn(char, &mut commands::Context),
    ) -> EventResult {
        match self.handle_keymap_event(event, ctx) {
            Some(KeymapResult::NotFound) => {
                if let KeyCode::Char(c) = event.code {
                    char_func(c, ctx);
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
                            char_func(c, ctx);
                            result = EventResult::Consumed(None);
                        }
                        _ => {
                            if let KeymapResult::Found(f) = self.keymaps.get(&ctx.editor.mode, event) {
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

const MAX_OFFSET_X: usize = 6;
const MAX_OFFSET_Y: usize = 3;
fn compute_offset(size: Rect) -> (usize, usize) {
    (
        ((size.width as usize).saturating_sub(1).max(1) / 2).min(MAX_OFFSET_X),
        ((size.height as usize).saturating_sub(1).max(1) / 2).min(MAX_OFFSET_Y),
    )
}


fn ensure_cursor_is_in_view(ctx: &mut Context) -> HashMap<PaneId, (Rect, Rect)> {
    let mut areas = HashMap::new();

    for (_, pane) in ctx.editor.panes.panes.iter_mut() {
        let doc = ctx.editor.documents.get(&pane.doc_id).expect("Can't get doc from pane id");
        let sel = doc.selection(pane.id);

        let gutter_area = gutter::area(pane.area, doc);

        let document_area = pane.area.clip_left(gutter_area.width);

        (pane.view.scroll.offset_x, pane.view.scroll.offset_y) = compute_offset(document_area);

        pane.view.scroll.ensure_cursor_is_in_view(&sel, &document_area);

        areas.insert(pane.id, (gutter_area, document_area));
    }

    areas
}

impl Component for EditorView {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        // clip 1 row from the bottom for status line
        ctx.editor.panes.resize(area.clip_bottom(1));

        // ensuring the cursor is in view needs to happen before obtaining
        // the view's visible byte range. This function also returns the
        // calculated areas for the gutter and document for each pane for
        // convenience
        let areas = ensure_cursor_is_in_view(ctx);

        // re-borrow as immutable
        let ctx = &*ctx;

        for (_, pane) in ctx.editor.panes.panes.iter() {
            let (gutter_area, document_area) = areas.get(&pane.id).unwrap();
            // render the view after ajusting the scroll cursor
            pane.view.render(pane, document_area, buffer, ctx);
            // and then the gutter
            gutter::render(pane, gutter_area, buffer, ctx);
        }

        ctx.editor.panes.draw_borders(buffer);
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
                Mode::Insert => self.handle_insert_mode_key_event(event, &mut action_ctx, actions::append_character),
                Mode::Replace => self.handle_insert_mode_key_event(event, &mut action_ctx, actions::append_or_replace_character),
                _ => self.handle_normal_mode_key_event(event, &mut action_ctx),
            }
        };

        self.on_next_key = action_ctx.on_next_key_callback.take();

        let callback = if action_ctx.compositor_callbacks.is_empty() {
            None
        } else {
            let cb: compositor::Callback = Box::new(|compositor, cx| {
                for compositor_cb in action_ctx.compositor_callbacks {
                    compositor_cb(compositor, cx);
                }
            });

            Some(cb)
        };

        // Escaping back to normal mode
        // merges the transactions and commits to history
        if ctx.editor.mode == Mode::Normal {
           current!(ctx.editor).1.commit_transaction_to_history();
        }

        match event_result {
            EventResult::Ignored(_) => EventResult::Ignored(callback),
            EventResult::Consumed(_) => EventResult::Consumed(callback),
        }
    }

    // This is a shit implementation just for lulz
    // and breaks everything because it doesn't use transactions
    // pls fix, kthxbye
    // Ok, disabling for now
    fn handle_paste(&mut self, _str: &str, _ctx: &mut Context) -> EventResult {
        // let (pane, doc) = current!(ctx.editor);
        // pane.view.insert_str_at_cursor(&mut doc.rope, str, &ctx.editor.mode);
        // doc.modified = true;
        EventResult::Consumed(None)
    }

    fn cursor(&self, _area: Rect, ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (
            Some(pane!(ctx.editor).view.scroll.cursor),
            Some(if self.waiting_for_input || self.on_next_key.is_some() {
                SetCursorStyle::BlinkingUnderScore
            } else {
                match ctx.editor.mode {
                    Mode::Normal | Mode::Select => SetCursorStyle::SteadyBlock,
                    Mode::Insert => SetCursorStyle::SteadyBar,
                    Mode::Replace => SetCursorStyle::SteadyUnderScore,
                }
            }),
        )
    }
}
