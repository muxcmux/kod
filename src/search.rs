use crossterm::cursor::SetCursorStyle;
use crossterm::event::{KeyCode, KeyEvent};
use smallvec::SmallVec;

use crate::components::status_line;
use crate::ui::Position;
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
    select_all_matches: bool,
}

impl Search {
    pub fn new(history_idx: usize, select_all_matches: bool) -> Self {
        Self {
            input: TextInput::empty(),
            history_idx,
            select_all_matches,
        }
    }

    pub fn with_value(history_idx: usize, value: &str) -> Self {
        Self {
            input: TextInput::with_value(value),
            history_idx,
            select_all_matches: false,
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
        ctx.editor.search.result = if self.select_all_matches {
            Some(select_matches(ctx))
        } else {
            Some(search(false, ctx))
        };

        match &ctx.editor.search.result {
            Some(result) => match result {
                SearchResult::Ok(selection) => {
                    let (pane, doc) = current!(ctx.editor);
                    doc.set_selection(pane.id, selection.transform(|range| range.move_to(&doc.rope, None, None, &ctx.editor.mode)));
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
        let (mut x, y, area) = status_line::position(area);
        buffer.clear(area);

        let style = if ctx.editor.search.focused {
            THEME.get("ui.text_input")
        } else {
            let s = THEME.get("ui.statusline");
            status_line::draw_background(area, buffer);
            s
        };

        x = status_line::draw_editor_mode(x, y, buffer, ctx);
        x = status_line::draw_left(if self.select_all_matches { "󱈄" } else { "󰍉" }, x, y, buffer, style);

        let input_size = area.clip_left(x);
        self.input.render(input_size, buffer, Some(style));

        // right-hand side
        let right = status_line::draw_cursor_count(area.right().saturating_sub(1), y, buffer, style, ctx);
        _ = status_line::draw_search_matches(right, y, buffer, style, ctx);
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
                    ctx.editor.search.focused = false;
                    ctx.editor.registers.push('/', ctx.editor.search.query.clone());
                    self.history_idx = ctx.editor.registers.get('/').unwrap().len() - 1;

                    match result {
                        SearchResult::Ok(_) => EventResult::Consumed(None),
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
                    ctx.editor.search.focused = false;
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

    fn cursor(&self, _area: Rect, ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        if !ctx.editor.search.focused {
            return (None, None)
        }
        (
            Some(self.input.scroll.cursor),
            Some(SetCursorStyle::SteadyBar),
        )
    }
}

pub enum SearchResult {
    Ok(Selection),
    InvalidRegex,
    Empty,
    NoQuery,
}

fn build_regex(str: &str) -> anyhow::Result<regex_cursor::engines::meta::Regex> {
    let case_insensitive = !str.chars().any(char::is_uppercase);

    Ok(regex_cursor::engines::meta::Builder::new()
        .syntax(
            regex_cursor::regex_automata::util::syntax::Config::new()
                .case_insensitive(case_insensitive)
                .multi_line(true),
        )
        .build(str)?)
}

pub fn search(backwards: bool, ctx: &mut Context) -> SearchResult {
    if ctx.editor.search.query.is_empty() {
        return SearchResult::NoQuery
    }

    match build_regex(&ctx.editor.search.query) {
        Ok(re) => {
            let (_, doc) = current!(ctx.editor);
            let haystack = regex_cursor::Input::new(RopeCursor::new(doc.rope.byte_slice(..)));

            let mut matches: Vec<_> = re.find_iter(haystack).collect();
            matches.sort_by_key(|a| a.start());

            if matches.is_empty() {
                return SearchResult::Empty
            }

            let sel = &ctx.editor.search.original_selection;

            let range = sel.primary().collapse_to_start();

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
            let new_range = selection::Range::from_byte_range(&doc.rope, from..to);

            SearchResult::Ok(Selection { ranges: SmallVec::from([new_range]), primary_index: 0 })
        },
        Err(_) => SearchResult::InvalidRegex
    }
}

fn select_matches(ctx: &mut Context) -> SearchResult {
    if ctx.editor.search.query.is_empty() {
        return SearchResult::NoQuery
    }

    match build_regex(&ctx.editor.search.query) {
        Ok(re) => {
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

        Err(_) => SearchResult::InvalidRegex
    }
}
