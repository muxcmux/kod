use std::collections::HashMap;
use crate::commands;
use crate::commands::actions::append_or_replace_string;
use crate::commands::actions::append_string;
use crate::commands::actions::ActionStatus;
use crate::compositor;
use crate::current;
use crate::document::DocumentId;
use crate::graphemes::NEW_LINE;
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
    commands::KeyCallback,
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
            match f(ctx) {
                Err(ActionStatus::Error(e)) => ctx.editor.set_error(e),
                Err(ActionStatus::Warning(e)) => ctx.editor.set_warning(e),
                _ => {}
            }
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
                match event.code {
                    KeyCode::Char(c) => {
                        ctx.editor.request_buffered_input(c);
                        EventResult::Consumed(None)
                    },
                    KeyCode::Enter => {
                        ctx.editor.request_buffered_input(NEW_LINE);
                        EventResult::Consumed(None)
                    },
                    KeyCode::Tab => todo!(),
                    _ => EventResult::Ignored(None)
                }
            }
            Some(KeymapResult::Cancelled(pending)) => {
                let mut result = EventResult::Ignored(None);
                for event in pending {
                    match event.code {
                        KeyCode::Char(c) => {
                            ctx.editor.request_buffered_input(c);
                            result = EventResult::Consumed(None);
                        },
                        KeyCode::Enter => {
                            ctx.editor.request_buffered_input(NEW_LINE);
                            result = EventResult::Consumed(None);
                        },
                        KeyCode::Tab => todo!(),
                        _ => {
                            if let KeymapResult::Found(f) = self.keymaps.get(&ctx.editor.mode, event) {
                                match f(ctx) {
                                    Err(ActionStatus::Error(e)) => ctx.editor.set_error(e),
                                    Err(ActionStatus::Warning(e)) => ctx.editor.set_warning(e),
                                    _ => {}
                                }
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

fn ensure_pane_cursors_are_in_view(ctx: &mut Context) -> HashMap<PaneId, (Rect, Rect)> {
    let mut areas = HashMap::new();

    for (_, pane) in ctx.editor.panes.panes.iter_mut() {
        let doc = ctx.editor.documents.get(&pane.doc_id).expect("Can't get doc from pane id");
        let sel = doc.selection(pane.id);

        let gutter_area = gutter::area(pane.area, doc);

        let document_area = pane.area.clip_left(gutter_area.width);

        pane.view.scroll.adjust_offset(&document_area, MAX_OFFSET_X, MAX_OFFSET_Y);
        pane.view.scroll.ensure_point_is_visible(sel.primary().head.x, sel.primary().head.y, &document_area, None);

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
        let areas = ensure_pane_cursors_are_in_view(ctx);

        // re-borrow as immutable
        let ctx = &*ctx;

        for (id, pane) in ctx.editor.panes.panes.iter() {
            // the id key in the hashmap has to be the same as the pane's id
            // otherwise there was a screw up somewhere when inserting the
            // pane
            debug_assert!(*id == pane.id);
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
                Mode::Insert => self.handle_insert_mode_key_event(event, &mut action_ctx),
                Mode::Replace => self.handle_insert_mode_key_event(event, &mut action_ctx),
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

    fn handle_buffered_input(&mut self, string: &str, ctx: &mut Context) -> EventResult {
        let mut action_ctx = commands::Context {
            editor: ctx.editor,
            compositor_callbacks: vec![],
            on_next_key_callback: None,
        };

        if let Err(status) = match action_ctx.editor.mode {
            Mode::Insert => append_string(string, &mut action_ctx),
            Mode::Replace => append_or_replace_string(string, &mut action_ctx),
            _ => Ok(()),
        } {
            match status {
                ActionStatus::Warning(cow) => action_ctx.editor.set_warning(cow),
                ActionStatus::Error(cow) => action_ctx.editor.set_error(cow),
                _ => {},
            }
        }

        EventResult::Consumed(None)
    }

    fn handle_paste(&mut self, string: &str, ctx: &mut Context) -> EventResult {
        let mut action_ctx = commands::Context {
            editor: ctx.editor,
            compositor_callbacks: vec![],
            on_next_key_callback: None,
        };

        if let Err(status) = match action_ctx.editor.mode {
            Mode::Replace => append_or_replace_string(string, &mut action_ctx),
            _ => append_string(string, &mut action_ctx),
        } {
            match status {
                ActionStatus::Warning(cow) => action_ctx.editor.set_warning(cow),
                ActionStatus::Error(cow) => action_ctx.editor.set_error(cow),
                _ => {},
            }
            return EventResult::Consumed(None)
        }

        if action_ctx.editor.mode != Mode::Insert {
            let (_, doc) = current!(action_ctx.editor);
            doc.commit_transaction_to_history();
        }

        if action_ctx.editor.mode == Mode::Select {
            action_ctx.editor.mode = Mode::Normal;
        }

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

    fn handle_focus_gained(&mut self, ctx: &mut Context) -> EventResult {
        let docs_were_changed = reload_changed_docs_in_view(ctx);

        let (pane, doc) = current!(ctx.editor);
        let pane_id = pane.id;
        let doc_id = doc.id;

        let (current_doc_changed, callback) = ctx.editor.sync_pane_changes(pane_id, doc_id);

        if !docs_were_changed && !current_doc_changed {
            EventResult::Ignored(None)
        } else {
            EventResult::Consumed(callback)
        }
    }
}

// Silently reload all documents in view
// which don't have local modifications and have
// been changed externally
fn reload_changed_docs_in_view(ctx: &mut Context) -> bool {
    let mut had_changes = false;

    let doc_ids = ctx.editor.panes.panes
        .values()
        .map(|pane| pane.doc_id)
        .collect::<Vec<DocumentId>>();

    for doc_id in doc_ids {
        let pane_ids = ctx.editor.doc_in_panes(ctx.editor.documents.get(&doc_id).unwrap().id);
        let doc = ctx.editor.documents.get_mut(&doc_id).unwrap();

        if doc.was_changed() && !doc.is_modified() {
            // Reload the document silently and reposition the cursors
            // doc.reload() might fail for various reasons here, e.g.
            // the file no longer exists, but because there are no local
            // modifications, we silently ignore those errors as they
            // will be handled when the given document is either saved,
            // or comes into view
            had_changes = true;
            if doc.reload().is_ok() {
                for pane_id in pane_ids {
                    let sel = doc.selection(pane_id);
                    doc.set_selection(pane_id, sel.transform(|r| {
                        r.move_to(&doc.rope, None, None, &ctx.editor.mode)
                    }))
                }
            }
        }
    }

    had_changes
}
