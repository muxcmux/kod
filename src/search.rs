use crossterm::event::{KeyCode, KeyEvent};
use regex_cursor::engines::meta::Regex;
use smallvec::SmallVec;

use crate::ui::{buffer::Buffer, text_input::TextInput, theme::THEME, Rect};
use crate::selection::{self, Selection};
use crate::rope::RopeCursor;
use crate::editor::Mode;
use crate::current;
use crate::compositor::{Component, Context, EventResult};

#[derive(Default)]
pub struct SearchState {
    pub focused: bool,
    pub total_matches: usize,
    pub current_match: usize,
    pub original_selection: Selection,
    pub result: Option<SearchResult>,
    pub query: String,
}

pub struct Search {
    input: TextInput,
    history_idx: usize,
}

impl Search {
    pub fn new(history_idx: usize) -> Self {
        Self {
            input: TextInput::empty(),
            history_idx,
        }
    }
}

impl Search {
    fn revert_position(&self, ctx: &mut Context) {
        let (pane, doc) = current!(ctx.editor);
        doc.set_selection(pane.id, ctx.editor.search.original_selection.clone());
    }

    fn search(&mut self, ctx: &mut Context) -> EventResult {
        let new_query = self.input.value();
        if new_query == ctx.editor.search.query {
            return EventResult::Consumed(None)
        }

        ctx.editor.search.query = new_query;
        ctx.editor.search.result = Some(search(false, ctx));

        match &ctx.editor.search.result {
            Some(result) => match result {
                SearchResult::Ok(selection) => {
                    let (pane, doc) = current!(ctx.editor);
                    doc.set_selection(pane.id, selection.clone());
                    EventResult::Consumed(None)
                },
                _ => {
                    self.revert_position(ctx);
                    EventResult::Consumed(None)
                }
            },
            _ => unreachable!()
        }
    }
}

impl Component for Search {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        buffer.clear(area.clip_top(area.height.saturating_sub(1)));

        let style = if ctx.editor.search.focused {
            "ui.text_input"
        } else {
            "ui.text_input.blur"
        };

        buffer.put_str("󰍉", area.left() + 1, area.bottom().saturating_sub(1), THEME.get(style));

        let input_size = area.clip_top(area.height.saturating_sub(1)).clip_left(3);

        if ctx.editor.search.focused {
            self.input.render(input_size, true, buffer);
        } else {
            buffer.put_str(&ctx.editor.search.query, area.left() + 3, area.bottom().saturating_sub(1), THEME.get("ui.text_input.blur"));
        }

        match ctx.editor.mode {
            Mode::Select => {
                let (pane, doc) = current!(ctx.editor);
                let sel = doc.selection(pane.id);
                if sel.ranges.len() > 1 {
                    let label = format!("[{} cursors]", sel.ranges.len());
                    let label_len = label.chars().count();
                    buffer.put_str(&label, area.right().saturating_sub(1 + label_len as u16), area.bottom().saturating_sub(1), THEME.get("ui.text_input"));
                }
            },
            _ => {
                if ctx.editor.search.total_matches > 0 {
                    let label = format!("{}/{}", ctx.editor.search.current_match + 1, ctx.editor.search.total_matches);
                    let label_len = label.chars().count();
                    buffer.put_str(&label, area.right().saturating_sub(1 + label_len as u16), area.bottom().saturating_sub(1), THEME.get("ui.text_input.blur"));
                }
            }
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        if !ctx.editor.search.focused {
            return match event.code {
                KeyCode::Esc => {
                    EventResult::Ignored(Some(Box::new(|comp, _| _ = comp.remove::<Search>())))
                },
                _ => EventResult::Ignored(None)
            }
        }

        match event.code {
            KeyCode::Esc => {
                ctx.editor.search.focused = false;
                self.revert_position(ctx);
                self.dismiss()
            }
            KeyCode::Enter => match &ctx.editor.search.result {
                Some(result) => {
                    ctx.editor.registers.push('/', ctx.editor.search.query.clone());
                    self.history_idx = ctx.editor.registers.get('/').unwrap().len() - 1;

                    match result {
                        SearchResult::Ok(_) => {
                            ctx.editor.search.focused = false;
                            if ctx.editor.mode == Mode::Select {
                                return self.dismiss()
                            }
                            EventResult::Consumed(None)
                        },
                        SearchResult::InvalidRegex => {
                            ctx.editor.set_error("Invalid search regex");
                            self.dismiss()
                        },
                        SearchResult::Empty => {
                            ctx.editor.set_warning(format!("No matches found: {}", ctx.editor.search.query));
                            self.dismiss()
                        },
                        SearchResult::NoQuery => {
                            ctx.editor.set_error("No search term");
                            self.dismiss()
                        }
                    }
                },
                None => {
                    ctx.editor.set_error("No search term");
                    self.dismiss()
                },
            }
            KeyCode::Up => {
                if let Some(value) = ctx.editor.registers.get_nth('/', self.history_idx.saturating_sub(1)) {
                    self.input.set_value(value);
                    self.input.move_cursor_to(Some(usize::MAX), None);
                    self.history_idx = self.history_idx.saturating_sub(1);
                }
                self.search(ctx)
            }
            KeyCode::Down => {
                match ctx.editor.registers.get_nth('/', self.history_idx + 1) {
                    Some(value) => {
                        self.input.set_value(value);
                        self.input.move_cursor_to(Some(usize::MAX), None);
                        self.history_idx += 1;
                    }
                    None => {
                        self.input.clear();
                    }
                }
                self.search(ctx)
            }
            _ => {
                self.input.handle_key_event(event);
                self.search(ctx)
            }
        }
    }
}

pub enum SearchResult {
    Ok(Selection),
    InvalidRegex,
    Empty,
    NoQuery,
}

pub fn search(backwards: bool, ctx: &mut Context) -> SearchResult {
    if ctx.editor.search.query.is_empty() {
        return SearchResult::NoQuery
    }

    match Regex::new(&ctx.editor.search.query) {
        Ok(re) => {
            match ctx.editor.mode {
                Mode::Select => select_matches(re, ctx),
                _ => find_and_move_to_match(re, backwards, ctx),
            }
        },
        Err(_) => SearchResult::InvalidRegex
    }
}

fn select_matches(re: Regex, ctx: &mut Context) -> SearchResult {
    let (_, doc) = current!(ctx.editor);
    let sel = &ctx.editor.search.original_selection;

    let mut ranges = SmallVec::with_capacity(sel.ranges.len());

    for range in sel.ranges.iter() {
        let byte_range = range.byte_range(&doc.rope, &ctx.editor.mode);
        let start = byte_range.start;
        let haystack = regex_cursor::Input::new(RopeCursor::new(doc.rope.byte_slice(byte_range)));

        let mut matches: Vec<_> = re.find_iter(haystack).collect();
        matches.sort_by_key(|a| a.start());

        for m in matches.iter() {
            let new_range = selection::Range::from_byte_range(&doc.rope, start + m.start()..start + m.end());
            ranges.push(new_range);
        }
    }

    if ranges.is_empty() {
        return SearchResult::Empty;
    }

    SearchResult::Ok(Selection { ranges, primary_index: 0 })
}

fn find_and_move_to_match(re: Regex, backwards: bool, ctx: &mut Context) -> SearchResult {
    let (_, doc) = current!(ctx.editor);
    let haystack = regex_cursor::Input::new(RopeCursor::new(doc.rope.byte_slice(..)));

    let mut matches: Vec<_> = re.find_iter(haystack).collect();
    matches.sort_by_key(|a| a.start());

    if matches.is_empty() {
        return SearchResult::Empty
    }
    let sel = &ctx.editor.search.original_selection;

    let range = sel.primary().collapse_to_head();

    let offset = range.byte_range(&doc.rope, &Mode::Normal).start;

    ctx.editor.search.total_matches = matches.len();

    if backwards {
        ctx.editor.search.current_match = matches.len() - 1;
        for (i, m) in matches.iter().enumerate().rev() {
            if m.start() < offset {
                ctx.editor.search.current_match = i;
                break;
            }
        }
    } else {
        ctx.editor.search.current_match = 0;
        for (i, m) in matches.iter().enumerate() {
            if m.start() > offset {
                ctx.editor.search.current_match = i;
                break;
            }
        }
    }

    let from = matches[ctx.editor.search.current_match].start();
    let to = matches[ctx.editor.search.current_match].end();
    let new_range = selection::Range::from_byte_range(&doc.rope, from..to).flip();

    SearchResult::Ok(Selection { ranges: SmallVec::from([new_range]), primary_index: 0 })
}
