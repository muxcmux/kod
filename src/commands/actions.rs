use std::borrow::Cow;
use std::str::FromStr;

use crop::{Rope, RopeSlice};
use crossterm::event::KeyCode;
use smartstring::{LazyCompact, SmartString};

use crate::components::files::Files;
use crate::graphemes::GraphemeCategory;
use crate::history::Change;
use crate::selection::{self, cursor_at_byte, Cursor};
use crate::textobject::{self, LongWords, LongWordsBackwards, TextObjectKind, Words, WordsBackwards};
use crate::{editor::Mode, graphemes::{self, line_width, NEW_LINE_STR}, panes::Direction, search::Search};

use super::{palette::Palette, Context};

// From helix:
// These are macros to make getting very nested fields in the `Editor` struct easier
// These are macros instead of functions because functions will have to take `&mut self`
// However, rust doesn't know that you only want a partial borrow instead of borrowing the
// entire struct which `&mut self` says.  This makes it impossible to do other mutable
// stuff to the struct because it is already borrowed. Because macros are expanded,
// this circumvents the problem because it is just like indexing fields by hand and then
// putting a `&mut` in front of it. This way rust can see that we are only borrowing a
// part of the struct and not the entire thing.

/// Get the current pane and document mutably as a tuple.
/// Returns `(&mut Pane, &mut Document)`
#[macro_export]
macro_rules! current {
    ($editor:expr) => {{
        let pane = $crate::pane_mut!($editor);
        let doc = $crate::doc_mut!($editor, &pane.doc_id);
        (pane, doc)
    }};
}

/// Get the current pane and document as immutable refs
#[macro_export]
macro_rules! current_ref {
    ($editor:expr) => {{
        let pane = $editor.panes.panes.get(&$editor.panes.focus).expect("Can't get focused pane");
        let doc = &$editor.documents[&pane.doc_id];
        (pane, doc)
    }};
}

/// Get the current document mutably.
/// Returns `&mut Document`
#[macro_export]
macro_rules! doc_mut {
    ($editor:expr, $id:expr) => {{
        $editor.documents.get_mut($id).unwrap()
    }};
    ($editor:expr) => {{
        $crate::current!(&$editor).1
    }};
}

/// Get the current pane mutably.
/// Returns `&mut Pane`
#[macro_export]
macro_rules! pane_mut {
    ($editor:expr, $id:expr) => {{
        $editor.panes.panes.get_mut($id).expect(format!("Couldn't get pane with id: {:?}", $id))
    }};
    ($editor:expr) => {{
        $editor.panes.panes.get_mut(&$editor.panes.focus).expect("Couldn't get focused pane")
    }};
}

/// Get the current pane immutably
/// Returns `&Pane`
#[macro_export]
macro_rules! pane {
    ($editor:expr, $id:expr) => {{
        $editor.panes.panes.get($id).expect(format!("Couldn't get pane with id: {:?}", $id))
    }};
    ($editor:expr) => {{
        $editor.panes.panes.get(&$editor.panes.focus).expect("Couldn't get focused pane")
    }};
}

/// Get an immutable reference to the current doc
#[macro_export]
macro_rules! doc {
    ($editor:expr, $id:expr) => {{
        &$editor.documents[$id]
    }};
    ($editor:expr) => {{
        $crate::current_ref!($editor).1
    }};
}

// macro_rules! info { ($string:expr) => { return Err(ActionStatus::Info($string.into())) } }
macro_rules! warn { ($string:expr) => { return Err(ActionStatus::Warning($string.into())) } }
macro_rules! err { ($string:expr) => { return Err(ActionStatus::Error($string.into())) } }

#[derive(Copy, Clone)]
pub enum GotoCharacterMove {
    Forward((char, usize)),
    Backward((char, usize)),
}

pub enum ActionStatus {
    Warning(Cow<'static, str>),
    Error(Cow<'static, str>),
    // signifies nothing happened
    Noop,
}

pub type ActionResult = Result<(), ActionStatus>;

fn hide_search(ctx: &mut Context) -> ActionResult {
    ctx.compositor_callbacks.push(Box::new(|comp, _| {
        comp.remove::<Search>();
    }));

    Ok(())
}

fn ensure_editable(ctx: &mut Context) -> ActionResult {
    let (_, doc) = current!(ctx.editor);

    if doc.readonly {
        warn!("Turn off readonly mode before editing");
    }

    Ok(())
}

fn enter_insert_mode(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;
    ctx.editor.mode = Mode::Insert;
    hide_search(ctx)
}

fn enter_insert_mode_relative_to_cursor(x: usize, ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;
    for _ in 0..x {
        move_right(ctx)?;
    }

    Ok(())
}

fn move_all_to(x: Option<usize>, y: Option<usize>, ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.transform(|range| {
        range.move_to(&doc.rope, x, y, &ctx.editor.mode)
    }));

    Ok(())
}

fn goto_character_forward_impl(c: char, offset: usize, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.transform(|range| {
        let mut col = 0;
        for g in doc.rope.line(range.head.y).graphemes() {
            if col > range.head.x + offset && g.starts_with(c) {
                return range.move_to(&doc.rope, Some(col.saturating_sub(offset)), None, &ctx.editor.mode);
            }
            col += graphemes::width(&g);
        }

        range
    }));
}

fn goto_character_backward_impl(c: char, offset: usize, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.transform(|range| {
        let mut col = line_width(&doc.rope, range.head.y);
        for g in doc.rope.line(range.head.y).graphemes().rev() {
            if col < range.head.x.saturating_sub(offset) && g.starts_with(c) {
                return range.move_to(&doc.rope, Some(col.saturating_sub(offset)), None, &ctx.editor.mode);
            }
            col -= graphemes::width(&g);
        }

        range
    }));
}

pub fn clean_state(ctx: &mut Context) -> ActionResult {
    // Leave only primary cursor
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.clone().into_single());
    Ok(())
}

pub fn command_palette(ctx: &mut Context) -> ActionResult {
    let palette = Box::new(Palette::new());
    ctx.push_component(palette);

    Ok(())
}

pub fn enter_normal_mode(ctx: &mut Context) -> ActionResult {
    if ctx.editor.mode != Mode::Select {
        move_left(ctx)?;
        ctx.editor.mode = Mode::Normal;
    } else {
        ctx.editor.mode = Mode::Normal;
        return move_all_to(None, None, ctx);
    }

    Ok(())
}

pub fn add_cursor_below(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let last = sel.ranges.last().unwrap();
    doc.set_selection(pane.id, sel.push(last.down(&doc.rope, &ctx.editor.mode)));

    Ok(())
}

pub fn add_cursor_above(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let first = sel.ranges.first().unwrap();
    doc.set_selection(pane.id, sel.push(first.up(&doc.rope, &ctx.editor.mode)));

    Ok(())
}

pub fn add_cursor_next_word(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let last = sel.ranges.last().unwrap();
    let next = range_from_looping_lines_forward(last, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
        goto_word_start_forward_impl(Words::new(slice), range, line, rope, slice, mode)
    })
    .unwrap_or(
        last.move_to(
            &doc.rope,
            Some(usize::MAX),
            Some(doc.rope.line_len().saturating_sub(1)),
            &ctx.editor.mode
        )
    );
    doc.set_selection(pane.id, sel.push(next));

    Ok(())
}

pub fn add_cursor_prev_word(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let first = sel.ranges.first().unwrap();
    let next = range_from_looping_lines_backward(first, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
        goto_word_start_backward_impl(WordsBackwards::new(slice), range, line, rope, slice, mode)
    })
    .unwrap_or(
        first.move_to(&doc.rope, Some(0), Some(0), &ctx.editor.mode)
    );
    doc.set_selection(pane.id, sel.push(next));

    Ok(())
}

pub fn enter_select_mode(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    ctx.editor.mode = Mode::Select;
    doc.set_selection(pane.id, sel.transform(|r| r.anchor()));

    Ok(())
}

pub fn expand_selection_to_whole_lines(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        if range.head > range.anchor {
            selection::Range {
                anchor: Cursor { x: 0, y: range.anchor.y },
                ..range.move_to(&doc.rope, Some(usize::MAX), None, &Mode::Select)
            }
        } else {
            selection::Range {
                head: Cursor { x: 0, y: range.head.y },
                anchor: range.flip().move_to(&doc.rope, Some(usize::MAX), None, &Mode::Select).head,
                sticky_x: 0,
            }
        }
    }));

    Ok(())
}

pub fn enter_replace_mode(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    ctx.editor.mode = Mode::Replace;
    hide_search(ctx)
}

pub fn enter_insert_mode_at_cursor(ctx: &mut Context) -> ActionResult {
    enter_insert_mode_relative_to_cursor(0, ctx)
}

pub fn enter_insert_mode_at_first_non_whitespace(ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;
    goto_line_first_non_whitespace(ctx)
}

pub fn enter_insert_mode_after_cursor(ctx: &mut Context) -> ActionResult {
    enter_insert_mode_relative_to_cursor(1, ctx)
}

pub fn enter_insert_mode_at_eol(ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;
    goto_eol(ctx)
}

pub fn move_left(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).transform(|r| r.left(&doc.rope, &ctx.editor.mode)));

    Ok(())
}

pub fn move_right(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).transform(|r| r.right(&doc.rope, &ctx.editor.mode)));

    Ok(())
}

pub fn move_up(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).transform(|r| r.up(&doc.rope, &ctx.editor.mode)));

    Ok(())
}

pub fn move_down(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).transform(|r| r.down(&doc.rope, &ctx.editor.mode)));

    Ok(())
}

pub fn half_page_up(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        let y = range.head.y.saturating_sub(half);
        range.move_to(&doc.rope, None, Some(y), &ctx.editor.mode)
    }));

    Ok(())
}

pub fn half_page_down(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        let y = range.head.y + half;
        range.move_to(&doc.rope, None, Some(y), &ctx.editor.mode)
    }));

    Ok(())
}

pub fn goto_first_line(ctx: &mut Context) -> ActionResult {
    move_all_to(None, Some(0), ctx)
}

pub fn goto_last_line(ctx: &mut Context) -> ActionResult {
    let (_, doc) = current!(ctx.editor);
    move_all_to(None, Some(doc.rope.line_len().saturating_sub(1)), ctx)
}

pub fn goto_line_first_non_whitespace(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.transform(|range| {
        for (i, g) in doc.rope.line(range.head.y).graphemes().enumerate() {
            if GraphemeCategory::from(&g) != GraphemeCategory::Whitespace {
                return range.move_to(&doc.rope, Some(i), None, &ctx.editor.mode);
            }
        }

        range
    }));

    Ok(())
}

pub fn goto_eol(ctx: &mut Context) -> ActionResult {
    move_all_to(Some(usize::MAX), None, ctx)
}

fn goto_word_start_forward_impl(
    words: impl Iterator<Item = textobject::Range>,
    range: &selection::Range,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<selection::Range> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line > range.head.y || range.head.x < word.start {
            return Some(range.move_to(rope, Some(word.start), Some(line), mode))
        }
    }

    None
}

fn goto_word_end_forward_impl(
    words: impl Iterator<Item = textobject::Range>,
    range: &selection::Range,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<selection::Range> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line > range.head.y || range.head.x < word.end {
            return Some(range.move_to(rope, Some(word.end), Some(line), mode))
        }
    }

    None
}

fn goto_word_start_backward_impl(
    words: impl Iterator<Item = textobject::Range>,
    range: &selection::Range,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<selection::Range> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line < range.head.y || range.head.x > word.start {
            return Some(range.move_to(rope, Some(word.start), Some(line), mode));
        }
    }

    None
}

fn goto_word_end_backward_impl(
    words: impl Iterator<Item = textobject::Range>,
    range: &selection::Range,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<selection::Range> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line < range.head.y || range.head.x > word.end {
            return Some(range.move_to(rope, Some(word.end), Some(line), mode));
        }
    }

    None
}

fn range_from_looping_lines_forward(
    range: &selection::Range,
    rope: &Rope,
    mode: &Mode,
    f: impl Fn(&selection::Range, usize, &Rope, RopeSlice<'_>, &Mode) -> Option<selection::Range>
) -> Option<selection::Range> {
    let mut line = range.head.y;

    while line < rope.line_len() {
        let slice = rope.line(line);

        if let Some(s) = f(range, line, rope, slice, mode) {
            return Some(s);
        }

        line += 1;
    }

    None
}

fn range_from_looping_lines_backward(
    range: &selection::Range,
    rope: &Rope,
    mode: &Mode,
    f: impl Fn(&selection::Range, usize, &Rope, RopeSlice<'_>, &Mode) -> Option<selection::Range>
) -> Option<selection::Range> {
    let mut line = range.head.y as isize;

    while line >= 0 {
        let l = line as usize;
        let slice = rope.line(l);

        if let Some(s) = f(range, l, rope, slice, mode) {
            return Some(s);
        }

        line -= 1;
    }

    None
}

pub fn goto_word_start_forward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_forward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_start_forward_impl(Words::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(
                &doc.rope,
                Some(usize::MAX),
                Some(doc.rope.line_len().saturating_sub(1)),
                &ctx.editor.mode
            )
        )
    }));

    Ok(())
}

pub fn goto_long_word_start_forward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_forward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_start_forward_impl(LongWords::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(
                &doc.rope,
                Some(usize::MAX),
                Some(doc.rope.line_len().saturating_sub(1)),
                &ctx.editor.mode
            )
        )
    }));

    Ok(())
}

pub fn goto_word_end_forward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_forward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_end_forward_impl(Words::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(
                &doc.rope,
                Some(usize::MAX),
                Some(doc.rope.line_len().saturating_sub(1)),
                &ctx.editor.mode
            )
        )
    }));

    Ok(())
}

pub fn goto_long_word_end_forward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_forward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_end_forward_impl(LongWords::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(
                &doc.rope,
                Some(usize::MAX),
                Some(doc.rope.line_len().saturating_sub(1)),
                &ctx.editor.mode
            )
        )
    }));

    Ok(())
}

pub fn goto_word_start_backward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_backward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_start_backward_impl(WordsBackwards::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(&doc.rope, Some(0), Some(0), &ctx.editor.mode)
        )
    }));

    Ok(())
}

pub fn goto_long_word_start_backward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_backward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_start_backward_impl(LongWordsBackwards::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(&doc.rope, Some(0), Some(0), &ctx.editor.mode)
        )
    }));

    Ok(())
}

pub fn goto_word_end_backward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_backward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_end_backward_impl(WordsBackwards::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(&doc.rope, Some(0), Some(0), &ctx.editor.mode)
        )
    }));

    Ok(())
}

pub fn goto_long_word_end_backward(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range_from_looping_lines_backward(&range, &doc.rope, &ctx.editor.mode, |range, line, rope, slice, mode| {
            goto_word_end_backward_impl(LongWordsBackwards::new(slice), range, line, rope, slice, mode)
        })
        .unwrap_or(
            range.move_to(&doc.rope, Some(0), Some(0), &ctx.editor.mode)
        )
    }));

    Ok(())
}

pub fn goto_character_forward(ctx: &mut Context) -> ActionResult {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.last_goto_character_move = Some(GotoCharacterMove::Forward((c, 0)));
            goto_character_forward_impl(c, 0, ctx);
        }
    });

    Ok(())
}

pub fn goto_until_character_forward(ctx: &mut Context) -> ActionResult {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.last_goto_character_move = Some(GotoCharacterMove::Forward((c, 1)));
            goto_character_forward_impl(c, 1, ctx);
        }
    });

    Ok(())
}

pub fn goto_character_backward(ctx: &mut Context) -> ActionResult {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.last_goto_character_move = Some(GotoCharacterMove::Backward((c, 1)));
            goto_character_backward_impl(c, 1, ctx);
        }
    });

    Ok(())
}

pub fn goto_until_character_backward(ctx: &mut Context) -> ActionResult {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.last_goto_character_move = Some(GotoCharacterMove::Backward((c, 0)));
            goto_character_backward_impl(c, 0, ctx);
        }
    });

    Ok(())
}

pub fn repeat_goto_character_next(ctx: &mut Context) -> ActionResult {
    if let Some(char_move) = ctx.editor.last_goto_character_move {
        match char_move {
            GotoCharacterMove::Forward((c, offset)) => goto_character_forward_impl(c, offset, ctx),
            GotoCharacterMove::Backward((c, offset)) => goto_character_backward_impl(c, offset, ctx),
        }
    }

    Ok(())
}

pub fn repeat_goto_character_prev(ctx: &mut Context) -> ActionResult {
    if let Some(char_move) = ctx.editor.last_goto_character_move {
        match char_move {
            GotoCharacterMove::Backward((c, offset)) => goto_character_forward_impl(c, 1 - offset, ctx),
            GotoCharacterMove::Forward((c, offset)) => goto_character_backward_impl(c, 1 - offset, ctx),
        }
    }

    Ok(())
}

pub fn undo(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    if let Some(sel) = doc.undo_redo(true) {
        doc.set_selection(pane.id, sel.transform(|range|
            range.move_to(&doc.rope, None, None, &ctx.editor.mode)
        ))
    }

    Ok(())
}

pub fn redo(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    if let Some(sel) = doc.undo_redo(false) {
        doc.set_selection(pane.id, sel.transform(|range|
            range.move_to(&doc.rope, None, None, &ctx.editor.mode)
        ))
    }

    Ok(())
}

fn insert_or_replace_buffered_string(
    string: SmartString<LazyCompact>,
    ctx: &mut Context,
    byte_range_fn: impl Fn(&selection::Range, &Rope) -> std::ops::Range<usize>,
) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id).clone();

    let mut changes = Vec::with_capacity(sel.ranges.len());
    for range in sel.ranges.iter() {
        let byte_range = byte_range_fn(range, &doc.rope);
        changes.push((byte_range, Some(string.clone())));
    }

    // Apply the changes to the doc, which returns the transaction.
    // Then use the transaction to find the bytes where insertions occured
    let mut byte_pos = vec![];
    if let Some(t) = doc.modify(changes, sel.clone()) {
        let mut byte = 0;
        for op in t.operations {
            match op {
                crate::history::Operation::Retain(i) => byte += i,
                crate::history::Operation::Insert(s) => {
                    byte += s.len();
                    byte_pos.push(byte);
                },
                _ => {}
            }
        }
    }

    // Reverse the byte positions and transform the cursors
    // with their heads at the new byte positions.
    // Cursors which overlap are collapsed.
    byte_pos.reverse();
    doc.set_selection(pane.id, sel.transform(|range| {
        let byte = byte_pos.pop().unwrap();
        let Cursor {x, y} = cursor_at_byte(&doc.rope, byte);
        let move_to_mode = match ctx.editor.mode {
            Mode::Select => &Mode::Normal,
            _ => &ctx.editor.mode
        };
        range.move_to(&doc.rope, Some(x), Some(y), move_to_mode)
    }));

    Ok(())
}

pub fn append_string(string: SmartString<LazyCompact>, ctx: &mut Context) -> ActionResult {
    let mode = ctx.editor.mode.clone();
    insert_or_replace_buffered_string(string, ctx, |range, rope| {
        range.byte_range(rope, &mode)
    })
}

pub fn append_or_replace_string(string: SmartString<LazyCompact>, ctx: &mut Context) -> ActionResult {
    let width = graphemes::width(&string);
    insert_or_replace_buffered_string(string, ctx, |range, rope| {
        range.move_to(rope, Some(range.head.x + width), None, &Mode::Select)
            .byte_range(rope, &Mode::Replace)
    })
}

pub fn insert_line_below(ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;

    insert_or_replace_buffered_string(SmartString::from_str(NEW_LINE_STR).unwrap(), ctx, |range, rope| {
        let offset = rope.byte_of_line(range.head.y) + rope.line(range.head.y).byte_len();
        offset..offset
    })
}

pub fn insert_line_above(ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;


    insert_or_replace_buffered_string(SmartString::from_str(NEW_LINE_STR).unwrap(), ctx, |range, rope| {
        let offset = rope.byte_of_line(range.head.y);
        offset..offset
    })?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.transform(|range| range.move_to(&doc.rope, Some(0), Some(range.head.y.saturating_sub(1)), &ctx.editor.mode)));

    Ok(())
}

fn delete_byte_ranges(
    ctx: &mut Context,
    byte_range_fn: impl Fn(&selection::Range, &Rope) -> Option<std::ops::Range<usize>>,
) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id).clone();

    let mut changes: Vec<Change> = Vec::with_capacity(sel.ranges.len());
    for range in sel.ranges.iter() {
        // When the byte_range_fn has nothing to delete, e.g. returns None, we push a dummy
        // deletion to the changes with a start and end byte equal to the cursor's start byte.
        // This allows us to keep the cursor visible even when it doesn't delete any text.
        let change = if let Some(byte_range) = byte_range_fn(range, &doc.rope) {
            (byte_range, None)
        } else {
            let byte_range = range.byte_range(&doc.rope, &ctx.editor.mode);
            (byte_range.start..byte_range.start, None)
        };
        // Assume the ranges are sorted and merge with the last one if overlaping
        match changes.last_mut() {
            Some((r, _)) if r.start == change.0.start || (r.end > change.0.start && change.0.end > r.start)  => {
                r.start = r.start.min(change.0.start);
                r.end = r.end.max(change.0.end);
            }
            _ => changes.push(change)
        }
    }

    // don't do anything if there are no deletes
    if changes.iter().all(|c| c.0.is_empty() && c.1.is_none()) {
        return Err(ActionStatus::Noop);
    }

    // Apply the changes to the doc, which returns the transaction.
    // Then use the transaction to find the bytes where deletions occured.
    let mut byte_pos = vec![];
    if let Some(t) = doc.modify(changes, sel.clone()) {
        let mut byte = 0;
        for op in t.operations {
            match op {
                crate::history::Operation::Retain(i) => byte += i,
                crate::history::Operation::Delete(_) => byte_pos.push(byte),
                _ => {}
            }
        }
    }

    // Reverse the byte positions and transform the cursors
    // with their heads at the new byte positions.
    // Cursors which delete the same ranges are collapsed.
    byte_pos.reverse();
    let mut last_range = *sel.primary();
    doc.set_selection(pane.id, sel.transform(|range| {
        if let Some(byte) = byte_pos.pop() {
            let Cursor {x, y} = cursor_at_byte(&doc.rope, byte);
            let move_to_mode = match ctx.editor.mode {
                Mode::Select => &Mode::Insert,
                _ => &ctx.editor.mode
            };
            let range = range.move_to(&doc.rope, Some(x), Some(y), move_to_mode);
            last_range = range;
            range
        } else {
            last_range
        }
    }));

    Ok(())
}

pub fn delete_symbol_to_the_left(ctx: &mut Context) -> ActionResult {
    delete_byte_ranges(ctx, |range, rope| {
        // sketchy AF
        let (x, y) = if range.head.x > 0 {
            (range.head.x - 1, range.head.y)
        } else if range.head.y > 0 {
            (usize::MAX, range.head.y - 1)
        } else {
            (0, 0)
        };

        Some(
            range.move_to(rope, Some(x), Some(y), &Mode::Select).byte_range(rope, &Mode::Insert)
        )
    })
}

pub fn delete_current_line(ctx: &mut Context) -> ActionResult {
    expand_selection_to_whole_lines(ctx)?;
    delete_selection_impl(ctx)
}

fn delete_text_object_inside_impl(ctx: &mut Context, enter_insert_mode: bool) -> ActionResult {
    ensure_editable(ctx)?;

    ctx.on_next_key(move |ctx, event| {
        if let Ok(kind) = TextObjectKind::try_from(event.code) {
            let deleted = delete_byte_ranges(ctx, |range, rope| {
                kind.inside(rope, range).map(|textobject::Range {start_byte, end_byte, ..}| {
                    let offset = rope.byte_of_line(range.head.y);
                    offset + start_byte..offset + end_byte
                })
            });
            if deleted.is_ok() && enter_insert_mode {
                _ = self::enter_insert_mode_relative_to_cursor(1, ctx);
            }
        }
    });

    Ok(())
}

pub fn delete_text_object_inside(ctx: &mut Context) -> ActionResult {
    delete_text_object_inside_impl(ctx, false)
}

pub fn delete_until_eol(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    delete_byte_ranges(ctx, |range, rope| {
        Some(range.anchor()
            .move_to(rope, Some(usize::MAX), None, &Mode::Select)
            .byte_range(rope, &Mode::Normal))
    })
}

pub fn change_until_eol(ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;
    delete_until_eol(ctx)
}

pub fn change_text_object_inside(ctx: &mut Context) -> ActionResult {
    delete_text_object_inside_impl(ctx, true)
}

pub fn change_current_line(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.set_selection(pane.id, sel.transform(|range| {
        range.move_to(&doc.rope, Some(0), None, &ctx.editor.mode)
    }));
    change_until_eol(ctx)
}

pub fn change_symbol_to_the_left(ctx: &mut Context) -> ActionResult {
    delete_symbol_to_the_left(ctx)?;
    enter_insert_mode(ctx)
}

pub fn switch_pane_top(ctx: &mut Context) -> ActionResult {
    ctx.editor.panes.switch(Direction::Up);
    hide_search(ctx)
}

pub fn switch_pane_bottom(ctx: &mut Context) -> ActionResult {
    ctx.editor.panes.switch(Direction::Down);
    hide_search(ctx)
}

pub fn switch_pane_left(ctx: &mut Context) -> ActionResult {
    ctx.editor.panes.switch(Direction::Left);
    hide_search(ctx)
}

pub fn switch_pane_right(ctx: &mut Context) -> ActionResult {
    ctx.editor.panes.switch(Direction::Right);
    hide_search(ctx)
}

pub fn switch_to_last_pane(ctx: &mut Context) -> ActionResult {
    ctx.editor.panes.switch_to_last();
    hide_search(ctx)
}

pub fn search(ctx: &mut Context) -> ActionResult {
    ctx.compositor_callbacks.push(Box::new(|comp, cx| {
        cx.editor.search.focused = true;
        cx.editor.search.total_matches = 0;
        cx.editor.search.current_match = 0;
        comp.remove::<Search>();
        let qhistory = cx.editor.search.query_history.clone();
        comp.push(Box::new(Search::new(qhistory)))
    }));

    Ok(())
}

pub fn next_search_match(ctx: &mut Context) -> ActionResult {
    if ctx.editor.search.query_history.is_empty() {
        err!("No search term present");
    } else {
        ctx.compositor_callbacks.push(Box::new(|comp, cx| {
            cx.editor.search.focused = false;
            crate::search::search(cx, false);
            comp.remove::<Search>();
            comp.push(Box::new(Search::with_term(cx.editor.search.query_history.last().unwrap())));
        }));
    }

    Ok(())
}

pub fn prev_search_match(ctx: &mut Context) -> ActionResult {
    if ctx.editor.search.query_history.is_empty() {
        err!("No search term present");
    } else {
        ctx.compositor_callbacks.push(Box::new(|comp, cx| {
            cx.editor.search.focused = false;
            crate::search::search(cx, true);
            comp.remove::<Search>();
            comp.push(Box::new(Search::with_term(cx.editor.search.query_history.last().unwrap())));
        }));
    }

    Ok(())
}

pub fn flip_selection(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.transform(|r| r.flip()));

    Ok(())
}

fn delete_selection_impl(ctx: &mut Context) -> ActionResult {
    delete_byte_ranges(ctx, |range, rope| {
        Some(range.byte_range(rope, &Mode::Select))
    })
}

pub fn delete_selection(ctx: &mut Context) -> ActionResult {
    delete_selection_impl(ctx)?;
    enter_normal_mode(ctx)
}

pub fn delete_selection_linewise(ctx: &mut Context) -> ActionResult {
    expand_selection_to_whole_lines(ctx)?;
    delete_selection(ctx)
}

pub fn change_selection(ctx: &mut Context) -> ActionResult {
    delete_selection_impl(ctx)?;
    enter_insert_mode(ctx)
}

pub fn change_selection_linewise(ctx: &mut Context) -> ActionResult {
    expand_selection_to_whole_lines(ctx)?;
    change_selection(ctx)
}

pub fn open_files(ctx: &mut Context) -> ActionResult {
    let (_, doc) = current!(ctx.editor);

    let files = Files::new(doc.path.as_ref())
        .map_err(|e| ActionStatus::Error(e.to_string().into()))?;

    ctx.push_component(Box::new(files));

    Ok(())
}
