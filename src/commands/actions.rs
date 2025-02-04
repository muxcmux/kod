use std::borrow::Cow;
use std::ops::Range;

use crop::{Rope, RopeSlice};
use crossterm::event::KeyCode;
use smartstring::SmartString;

use crate::components::files::Files;
use crate::graphemes::GraphemeCategory;
use crate::selection::Cursor;
use crate::textobject::{self, LongWords, LongWordsBackwards, TextObjectKind, Words, WordsBackwards};
use crate::{document::Document, editor::Mode, graphemes::{self, line_width, NEW_LINE, NEW_LINE_STR}, panes::Direction, search::Search, selection::Selection};

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
        cursor_right(ctx)?;
    }

    Ok(())
}

fn move_cursor_to(x: Option<usize>, y: Option<usize>, ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let selection = doc.selection(pane.id);
    doc.set_selection(pane.id, selection.head_to(&doc.rope, x, y, &ctx.editor.mode));

    Ok(())
}

fn goto_character_forward_impl(c: char, offset: usize, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let mut sel = doc.selection(pane.id);
    let mut col = 0;
    for g in doc.rope.line(sel.head.y).graphemes() {
        if col > sel.head.x + offset && g.starts_with(c) {
            sel = sel.head_to(&doc.rope, Some(col.saturating_sub(offset)), None, &ctx.editor.mode);
            break;
        }
        col += graphemes::width(&g);
    }

    doc.set_selection(pane.id, sel);
}

fn goto_character_backward_impl(c: char, offset: usize, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let mut sel = doc.selection(pane.id);
    let mut col = line_width(&doc.rope, sel.head.y);
    for g in doc.rope.line(sel.head.y).graphemes().rev() {
        if col < sel.head.x.saturating_sub(offset) && g.starts_with(c) {
            sel = sel.head_to(&doc.rope, Some(col.saturating_sub(offset)), None, &ctx.editor.mode);
            break;
        }
        col -= graphemes::width(&g);
    }

    doc.set_selection(pane.id, sel);
}

pub fn command_palette(ctx: &mut Context) -> ActionResult {
    let palette = Box::new(Palette::new());
    ctx.push_component(palette);

    Ok(())
}

pub fn enter_normal_mode(ctx: &mut Context) -> ActionResult {
    if ctx.editor.mode != Mode::Select {
        cursor_left(ctx)?;
        ctx.editor.mode = Mode::Normal;
    } else {
        ctx.editor.mode = Mode::Normal;
        return move_cursor_to(None, None, ctx);
    }

    Ok(())
}

pub fn enter_select_mode(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    ctx.editor.mode = Mode::Select;
    doc.set_selection(pane.id, sel.anchor());

    Ok(())
}

pub fn expand_selection_to_whole_lines(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    let expanded = if sel.head > sel.anchor {
        Selection {
            anchor: Cursor { x: 0, y: sel.anchor.y },
            ..sel.head_to(&doc.rope, Some(usize::MAX), None, &ctx.editor.mode)
        }
    } else {
        Selection {
            head: Cursor { x: 0, y: sel.head.y },
            anchor: sel.invert().head_to(&doc.rope, Some(usize::MAX), None, &ctx.editor.mode).head,
            sticky_x: 0,
        }
    };

    doc.set_selection(pane.id, expanded);

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

pub fn cursor_left(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).left(&doc.rope, &ctx.editor.mode));

    Ok(())
}

pub fn cursor_right(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).right(&doc.rope, &ctx.editor.mode));

    Ok(())
}

pub fn cursor_up(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).up(&doc.rope, &ctx.editor.mode));

    Ok(())
}

pub fn cursor_down(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).down(&doc.rope, &ctx.editor.mode));

    Ok(())
}

pub fn half_page_up(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let y = doc.selection(pane.id).head.y.saturating_sub(half);
    move_cursor_to(None, Some(y), ctx)
}

pub fn half_page_down(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let y = doc.selection(pane.id).head.y + half;
    move_cursor_to(None, Some(y), ctx)
}

pub fn goto_first_line(ctx: &mut Context) -> ActionResult {
    move_cursor_to(None, Some(0), ctx)
}

pub fn goto_last_line(ctx: &mut Context) -> ActionResult {
    let (_, doc) = current!(ctx.editor);
    move_cursor_to(None, Some(doc.rope.line_len().saturating_sub(1)), ctx)
}

pub fn goto_line_first_non_whitespace(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    for (i, g) in doc.rope.line(sel.head.y).graphemes().enumerate() {
        if GraphemeCategory::from(&g) != GraphemeCategory::Whitespace {
            doc.set_selection(
                pane.id,
                sel.head_to(&doc.rope, Some(i), Some(sel.head.y), &ctx.editor.mode),
            );
            break;
        }
    }

    Ok(())
}

pub fn goto_eol(ctx: &mut Context) -> ActionResult {
    move_cursor_to(Some(usize::MAX), None, ctx)
}

fn set_selection_or(
    sel: Option<Selection>,
    ctx: &mut Context,
    f: impl FnOnce(Selection, &Rope, &Mode) -> Selection,
) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);

    doc.set_selection(
        pane.id,
        sel.unwrap_or(
            f(doc.selection(pane.id), &doc.rope, &ctx.editor.mode)
        )
    );

    Ok(())
}

fn goto_word_start_forward_impl(
    words: impl Iterator<Item = textobject::Range>,
    sel: &Selection,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<Selection> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line > sel.head.y || sel.head.x < word.start {
            return Some(sel.head_to(rope, Some(word.start), Some(line), mode))
        }
    }

    None
}

fn goto_word_end_forward_impl(
    words: impl Iterator<Item = textobject::Range>,
    sel: &Selection,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<Selection> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line > sel.head.y || sel.head.x < word.end {
            return Some(sel.head_to(rope, Some(word.end), Some(line), mode))
        }
    }

    None
}

fn goto_word_start_backward_impl(
    words: impl Iterator<Item = textobject::Range>,
    sel: &Selection,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<Selection> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line < sel.head.y || sel.head.x > word.start {
            return Some(sel.head_to(rope, Some(word.start), Some(line), mode));
        }
    }

    None
}

fn goto_word_end_backward_impl(
    words: impl Iterator<Item = textobject::Range>,
    sel: &Selection,
    line: usize,
    rope: &Rope,
    slice: RopeSlice<'_>,
    mode: &Mode,
) -> Option<Selection> {
    for word in words {
        if word.is_blank(slice) { continue; }

        if line < sel.head.y || sel.head.x > word.end {
            return Some(sel.head_to(rope, Some(word.end), Some(line), mode));
        }
    }

    None
}

fn selection_from_looping_lines_forward(
    ctx: &mut Context,
    f: impl Fn(&Selection, usize, &Rope, RopeSlice<'_>, &Mode) -> Option<Selection>
) -> Option<Selection> {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let mut line = sel.head.y;

    while line < doc.rope.line_len() {
        let slice = doc.rope.line(line);

        if let Some(s) = f(&sel, line, &doc.rope, slice, &ctx.editor.mode) {
            return Some(s);
        }

        line += 1;
    }

    None
}

fn selection_from_looping_lines_backward(
    ctx: &mut Context,
    f: impl Fn(&Selection, usize, &Rope, RopeSlice<'_>, &Mode) -> Option<Selection>
) -> Option<Selection> {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let mut line = sel.head.y as isize;

    while line >= 0 {
        let l = line as usize;
        let slice = doc.rope.line(l);

        if let Some(s) = f(&sel, l, &doc.rope, slice, &ctx.editor.mode) {
            return Some(s);
        }

        line -= 1;
    }

    None
}

pub fn goto_word_start_forward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_forward(ctx, |sel, line, rope, slice, mode| {
        goto_word_start_forward_impl(Words::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(usize::MAX), Some(rope.line_len().saturating_sub(1)), mode)
    })
}

pub fn goto_long_word_start_forward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_forward(ctx, |sel, line, rope, slice, mode| {
        goto_word_start_forward_impl(LongWords::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(usize::MAX), Some(rope.line_len().saturating_sub(1)), mode)
    })
}

pub fn goto_word_end_forward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_forward(ctx, |sel, line, rope, slice, mode| {
        goto_word_end_forward_impl(Words::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(usize::MAX), Some(rope.line_len().saturating_sub(1)), mode)
    })
}

pub fn goto_long_word_end_forward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_forward(ctx, |sel, line, rope, slice, mode| {
        goto_word_end_forward_impl(LongWords::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(usize::MAX), Some(rope.line_len().saturating_sub(1)), mode)
    })
}

pub fn goto_word_start_backward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_backward(ctx, |sel, line, rope, slice, mode| {
        goto_word_start_backward_impl(WordsBackwards::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(0), Some(0), mode)
    })
}

pub fn goto_long_word_start_backward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_backward(ctx, |sel, line, rope, slice, mode| {
        goto_word_start_backward_impl(LongWordsBackwards::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(0), Some(0), mode)
    })
}

pub fn goto_word_end_backward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_backward(ctx, |sel, line, rope, slice, mode| {
        goto_word_end_backward_impl(WordsBackwards::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(0), Some(0), mode)
    })
}

pub fn goto_long_word_end_backward(ctx: &mut Context) -> ActionResult {
    let sel = selection_from_looping_lines_backward(ctx, |sel, line, rope, slice, mode| {
        goto_word_end_backward_impl(LongWordsBackwards::new(slice), sel, line, rope, slice, mode)
    });

    set_selection_or(sel, ctx, |sel, rope, mode| {
        sel.head_to(rope, Some(0), Some(0), mode)
    })
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
        doc.set_selection(pane.id, sel.head_to(&doc.rope, None, None, &ctx.editor.mode))
    }

    Ok(())
}

pub fn redo(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    if let Some(sel) = doc.undo_redo(false) {
        doc.set_selection(pane.id, sel.head_to(&doc.rope, None, None, &ctx.editor.mode))
    }

    Ok(())
}

fn insert_or_replace_char(c: char, range: Range<usize>, selection: Option<Selection>, ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;
    let (pane, doc) = current!(ctx.editor);
    let mut string = SmartString::new();
    string.push(c);

    let start = range.start;

    doc.modify((range, Some(string)), doc.selection(pane.id));

    move_cursor_after_appending_or_replacing_character(c, start, selection, ctx)
}

pub fn append_character(c: char, ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let range = doc.selection(pane.id)
        .collapse_to_head()
        .byte_range(&doc.rope, false, false);
    insert_or_replace_char(c, range, None, ctx)
}

fn move_cursor_after_appending_or_replacing_character(c: char, offset: usize, move_to: Option<Selection>, ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    match c {
        NEW_LINE => {
            doc.set_selection(pane.id, move_to.unwrap_or(sel.head_to(&doc.rope, Some(0), Some(sel.head.y + 1), &ctx.editor.mode)));
        }
        '\u{200d}' => {
            // if the current or previous chars are zero-width
            // joiners we don't move the cursor to the right
            let zwj_bytes: [u8; 3] = [226, 128, 141];
            let prev_bytes = [
                doc.rope.byte(offset.saturating_sub(3)),
                doc.rope.byte(offset.saturating_sub(2)),
                doc.rope.byte(offset.saturating_sub(1))
            ];
            if prev_bytes != zwj_bytes {
                return cursor_right(ctx)
            }
        }
        _ => return cursor_right(ctx)
    }

    Ok(())
}

pub fn append_or_replace_character(c: char, ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id).collapse_to_head();

    insert_or_replace_char(c, sel.byte_range(&doc.rope, true, false), None, ctx)
}

pub fn append_new_line(ctx: &mut Context) -> ActionResult {
    append_character(NEW_LINE, ctx)
}

pub fn insert_line_below(ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let offset = doc.rope.byte_of_line(sel.head.y) + doc.rope.line(sel.head.y).byte_len();
    insert_or_replace_char(NEW_LINE, offset..offset, None, ctx)
}

pub fn insert_line_above(ctx: &mut Context) -> ActionResult {
    enter_insert_mode(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let offset = doc.rope.byte_of_line(sel.head.y);
    insert_or_replace_char(NEW_LINE, offset..offset, Some(sel.head_to(&doc.rope, Some(0), None, &ctx.editor.mode)), ctx)
}

fn delete_to_the_left(rope: &Rope, sel: Selection, mode: &Mode) -> Option<(Range<usize>, Selection)> {
    if sel.head.x > 0 {
        let new_sel = sel.left(rope, mode).collapse_to_head();
        let range = new_sel.byte_range(rope, true, true);

        return Some((range, new_sel));
    } else if sel.head.y > 0  {
        // Using Mode::Select here, because it can move past the end of last grapheme
        let new_sel = sel.head_to(rope, Some(usize::MAX), Some(sel.head.y - 1), &Mode::Select).collapse_to_head();
        let range = new_sel.byte_range(rope, true, true);

        return Some((range, new_sel.head_to(rope, None, None, mode)));
    }

    None
}

pub fn delete_symbol_to_the_left(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    if let Some((range, sel)) = delete_to_the_left(&doc.rope, sel, &ctx.editor.mode) {
        doc.set_selection(pane.id, sel);
        doc.modify((range, None), sel);
    }

    Ok(())
}

fn delete_lines(sel: Selection, size: usize, doc: &mut Document) -> bool {
    let from = sel.head.y;
    let rope = &mut doc.rope;

    if rope.is_empty() || rope == NEW_LINE_STR { return false }

    let to = (from + size).min(rope.line_len());

    let s = rope.line_slice(from..to);

    let start = rope.byte_of_line(from);
    let mut end = start + s.byte_len();

    // if we are deleting everything, remember to leave the newline byte
    if start == 0 && to == rope.line_len() {
        end = rope.line(to).byte_len();
    }

    doc.modify((start..end, None), sel);

    true
}

pub fn delete_current_line(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    if delete_lines(sel, 1, doc) {
        if sel.head.y > doc.rope.line_len().saturating_sub(1) {
            return cursor_up(ctx)
        } else {
            return move_cursor_to(None, None, ctx)
        }
    }

    Ok(())
}

fn delete_text_object_inside_impl(ctx: &mut Context, enter_insert_mode: bool) -> ActionResult {
    ensure_editable(ctx)?;

    ctx.on_next_key(move |ctx, event| {
        if let Ok(kind) = TextObjectKind::try_from(event.code) {
            let (pane, doc) = current!(ctx.editor);
            let sel = doc.selection(pane.id);
            let textobject::Range { start, start_byte, end_byte, .. } = kind.inside(&doc.rope, &sel);
            let offset = doc.rope.byte_of_line(sel.head.y);
            doc.modify((offset + start_byte..offset + end_byte, None), sel);
            if enter_insert_mode {
                _ = self::enter_insert_mode(ctx);
            }
            _ = move_cursor_to(Some(start), None, ctx);
        }
    });

    Ok(())
}

pub fn delete_text_object_inside(ctx: &mut Context) -> ActionResult {
    delete_text_object_inside_impl(ctx, false)
}

pub fn delete_until_eol(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let range = sel.anchor()
        .head_to(&doc.rope, Some(usize::MAX), None, &Mode::Select)
        .byte_range(&doc.rope, true, false);
    if range.start > 0 {
        doc.modify((range, None), sel);
        return move_cursor_to(None, None, ctx)
    }

    Ok(())
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

    move_cursor_to(Some(0), None, ctx)?;
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

pub fn invert_selection(ctx: &mut Context) -> ActionResult {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.invert());

    Ok(())
}

fn delete_selection_impl(ctx: &mut Context) -> ActionResult {
    ensure_editable(ctx)?;

    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);

    doc.modify((sel.byte_range(&doc.rope, true, true), None), sel);

    doc.set_selection(pane.id, sel.collapse_to_smaller_end());

    Ok(())
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
