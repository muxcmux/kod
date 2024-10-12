use crate::commands;
use crate::compositor;
use crate::current;
use crate::pane;
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
}

impl EditorView {
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

    fn handle_insert_or_replace_mode_key_event(
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

impl Component for EditorView {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        ctx.editor.panes.resize(area);
        let mode = &ctx.editor.mode;
        for (_, pane) in ctx.editor.panes.panes.iter_mut() {
            let doc = ctx.editor.documents.get(&pane.doc_id).expect("Can't get doc from pane id");
            pane.render(buffer, doc, mode);
        }
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
                Mode::Insert => self.handle_insert_or_replace_mode_key_event(event, &mut action_ctx, actions::append_character),
                Mode::Replace => self.handle_insert_or_replace_mode_key_event(event, &mut action_ctx, actions::append_or_replace_character),
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

        // Escaping back to normal mode from insert and replace mode
        // merges the transactions and commits to history
        if ctx.editor.mode == Mode::Normal {
           current!(ctx.editor).1.commit_transaction_to_history();
        }

        match event_result {
            EventResult::Ignored(_) => EventResult::Ignored(callback),
            EventResult::Consumed(_) => EventResult::Consumed(callback),
        }
    }

    fn handle_paste(&mut self, str: &str, ctx: &mut Context) -> EventResult {
        let doc = current!(ctx.editor).1;
        doc.text.insert_str_at_cursor(str, &ctx.editor.mode);
        doc.modified = true;
        EventResult::Consumed(None)
    }

    fn cursor(&self, _area: Rect, ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (
            Some(pane!(ctx.editor).scroll_view.cursor_position),
            Some(match ctx.editor.mode {
                Mode::Normal => SetCursorStyle::SteadyBlock,
                Mode::Insert => SetCursorStyle::SteadyBar,
                Mode::Replace => SetCursorStyle::SteadyUnderScore,
            }),
        )
    }
}
