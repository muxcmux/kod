use crop::Rope;
use crossterm::event::KeyCode;
use smartstring::SmartString;

use crate::{document::Document, editor::Mode, graphemes::{self, line_width, NEW_LINE, NEW_LINE_STR}, history::Transaction, panes::Direction, search::Search, selection::Selection};

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

fn hide_search(ctx: &mut Context) {
    ctx.compositor_callbacks.push(Box::new(|comp, _| {
        comp.remove::<Search>();
    }));
}

fn enter_insert_mode(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    hide_search(ctx);
}

fn enter_insert_mode_relative_to_cursor(x: usize, ctx: &mut Context) {
    enter_insert_mode(ctx);
    for _ in 0..x {
        cursor_right(ctx);
    }
}

fn byte_range_until_eol(rope: &Rope, selection: &Selection) -> Option<(usize, usize)> {
    let start = selection.byte_offset_at_head(rope);
    let end = rope.byte_of_line(selection.head.y) + rope.line(selection.head.y).byte_len();

    if end > 0 {
        return Some((start, end));
    }

    None
}

fn move_cursor_to(x: Option<usize>, y: Option<usize>, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let selection = doc.selection(pane.id);
    doc.set_selection(pane.id, selection.move_to(&doc.rope, x, y, &ctx.editor.mode));
}

fn goto_character_forward_impl(c: char, offset: usize, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let mut sel = doc.selection(pane.id);
    let mut col = 0;
    for g in doc.rope.line(sel.head.y).graphemes() {
        if col > sel.head.x && g.starts_with(c) {
            sel = sel.move_to(&doc.rope, Some(col.saturating_sub(offset)), None, &ctx.editor.mode);
            break;
        }
        let width = graphemes::width(&g);
        col += width;
    }

    doc.set_selection(pane.id, sel);
}

fn goto_character_backward_impl(c: char, offset: usize, ctx: &mut Context) -> Selection {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let mut col = line_width(&doc.rope, sel.head.y);
    for g in doc.rope.line(sel.head.y).graphemes().rev() {
        if col <= sel.head.x && g.starts_with(c) {
            return sel.move_to(&doc.rope, Some(col.saturating_sub(offset)), None, &ctx.editor.mode);
        }
        let width = graphemes::width(&g);
        col -= width;
    }

    sel
}

pub fn command_palette(ctx: &mut Context) {
    let palette = Box::new(Palette::new());
    ctx.push_component(palette);
}

pub fn enter_normal_mode(ctx: &mut Context) {
    if ctx.editor.mode != Mode::Select {
        cursor_left(ctx);
    }

    ctx.editor.mode = Mode::Normal;
}

pub fn enter_select_mode(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.anchor());
    ctx.editor.mode = Mode::Select;
}

pub fn enter_replace_mode(ctx: &mut Context) {
    ctx.editor.mode = Mode::Replace;
    hide_search(ctx);
}

pub fn enter_insert_mode_at_cursor(ctx: &mut Context) {
    enter_insert_mode_relative_to_cursor(0, ctx);
}

pub fn enter_insert_mode_at_first_non_whitespace(ctx: &mut Context) {
    enter_insert_mode(ctx);
    goto_line_first_non_whitespace(ctx);
}

pub fn enter_insert_mode_after_cursor(ctx: &mut Context) {
    enter_insert_mode_relative_to_cursor(1, ctx);
}

pub fn enter_insert_mode_at_eol(ctx: &mut Context) {
    enter_insert_mode(ctx);
    goto_eol(ctx);
}

pub fn cursor_left(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).left(&doc.rope, &ctx.editor.mode));
}

pub fn cursor_right(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).right(&doc.rope, &ctx.editor.mode));
}

pub fn cursor_up(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).up(&doc.rope, &ctx.editor.mode));
}

pub fn cursor_down(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    doc.set_selection(pane.id, doc.selection(pane.id).down(&doc.rope, &ctx.editor.mode));
}

pub fn half_page_up(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let y = doc.selection(pane.id).head.y.saturating_sub(half);
    move_cursor_to(None, Some(y), ctx);
}

pub fn half_page_down(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let y = doc.selection(pane.id).head.y + half;
    move_cursor_to(None, Some(y), ctx);
}

pub fn goto_first_line(ctx: &mut Context) {
    move_cursor_to(None, Some(0), ctx);
}

pub fn goto_last_line(ctx: &mut Context) {
    let (_, doc) = current!(ctx.editor);
    move_cursor_to(None, Some(doc.rope.line_len().saturating_sub(1)), ctx);
}

pub fn goto_line_first_non_whitespace(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.goto_line_first_non_whitespace(&doc.rope, None, &ctx.editor.mode));
}

pub fn goto_eol(ctx: &mut Context) {
    move_cursor_to(Some(usize::MAX), None, ctx);
}

pub fn goto_word_start_forward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.goto_word_start_forward(&doc.rope, &ctx.editor.mode));
}

pub fn goto_word_end_forward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.goto_word_end_forward(&doc.rope, &ctx.editor.mode));
}

pub fn goto_word_start_backward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.goto_word_start_backward(&doc.rope, &ctx.editor.mode));
}

pub fn goto_word_end_backward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.goto_word_end_backward(&doc.rope, &ctx.editor.mode));
}

pub fn goto_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            goto_character_forward_impl(c, 0, ctx);
        }
    })
}

pub fn goto_until_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            goto_character_forward_impl(c, 1, ctx);
        }
    })
}

pub fn goto_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            goto_character_backward_impl(c, 1, ctx);
        }
    })
}

pub fn goto_until_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            goto_character_backward_impl(c, 0, ctx);
        }
    })
}

pub fn undo(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    if let Some(sel) = doc.undo_redo(true) {
        doc.set_selection(pane.id, sel)
    }
}

pub fn redo(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    if let Some(sel) = doc.undo_redo(false) {
        doc.set_selection(pane.id, sel)
    }
}

fn insert_or_replace_char_at_offset(c: char, offset_start: usize, offset_end: usize, selection: Option<Selection>, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let mut string = SmartString::new();
    string.push(c);

    doc.apply(
        &Transaction::change(
            &doc.rope,
            [(offset_start, offset_end, Some(string))].into_iter()
        ).set_selection(doc.selection(pane.id))
    );

    doc.modified = true;

    move_cursor_after_appending_or_replacing_character(c, offset_start, selection, ctx);
}

pub fn append_character(c: char, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let offset = doc.selection(pane.id).byte_offset_at_head(&doc.rope);
    insert_or_replace_char_at_offset(c, offset, offset, None, ctx);
}

fn move_cursor_after_appending_or_replacing_character(c: char, offset: usize, move_to: Option<Selection>, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    match c {
        NEW_LINE => {
            doc.set_selection(pane.id, move_to.unwrap_or(sel.move_to(&doc.rope, Some(0), Some(sel.head.y + 1), &ctx.editor.mode)));
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
                cursor_right(ctx);
            }
        }
        _ => cursor_right(ctx),
    }
}

pub fn append_or_replace_character(c: char, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let mut start_byte = doc.rope.byte_of_line(sel.head.y);
    let mut end_byte = start_byte;

    let mut col = 0;

    for g in doc.rope.line(sel.head.y).graphemes() {
        let width = graphemes::width(&g);
        let size = g.bytes().count();

        if col >= sel.head.x {
            end_byte = start_byte + size;
            break;
        }

        col += width;
        start_byte += size;
    }

    insert_or_replace_char_at_offset(c, start_byte, end_byte.max(start_byte), None, ctx);
}

pub fn append_new_line(ctx: &mut Context) {
    append_character(NEW_LINE, ctx);
}

pub fn insert_line_below(ctx: &mut Context) {
    enter_insert_mode(ctx);
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let offset = doc.rope.byte_of_line(sel.head.y) + doc.rope.line(sel.head.y).byte_len();
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, None, ctx);
}

pub fn insert_line_above(ctx: &mut Context) {
    enter_insert_mode(ctx);
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    let offset = doc.rope.byte_of_line(sel.head.y);
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, Some(sel.move_to(&doc.rope, Some(0), None, &ctx.editor.mode)), ctx);
}

fn delete_to_the_left(rope: &Rope, sel: Selection, mode: &Mode) -> Option<(usize, usize, Selection)> {
    if sel.head.x > 0 {
        let mut start = rope.byte_of_line(sel.head.y);
        let mut end = start;
        let idx = sel.grapheme_at_head(rope).0 - 1;

        for (i, g) in rope.line(sel.head.y).graphemes().enumerate() {
            if i < idx { start += g.len() }
            if i == idx {
                end = start + g.len();
                break
            }
        }

        return Some((start, end, sel.left(rope, mode)));

    } else if sel.head.y > 0  {
        let to = rope.byte_of_line(sel.head.y);
        let from = to.saturating_sub(NEW_LINE.len_utf8());

        return Some((from, to, sel.move_to(rope, Some(line_width(rope, sel.head.y - 1)), Some(sel.head.y - 1), mode)));
    }

    None
}

pub fn delete_symbol_to_the_left(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    if let Some((from, to, sel)) = delete_to_the_left(&doc.rope, sel, &ctx.editor.mode) {
        doc.set_selection(pane.id, sel);
        doc.apply(
            &Transaction::change(
                &doc.rope,
                [(from, to, None)].into_iter()
            ).set_selection(sel)
        );
        doc.modified = true;
    }
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
        end -= NEW_LINE.len_utf8();
    }

    let t = Transaction::change(rope,
        [(start, end, None)].into_iter()
    ).set_selection(sel);

    doc.apply(&t);

    true
}

pub fn delete_current_line(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    if delete_lines(sel, 1, doc) {
        doc.modified = true;
        if sel.head.y > doc.rope.line_len().saturating_sub(1) {
            cursor_up(ctx);
        } else {
            move_cursor_to(None, None, ctx);
        }
    }
}

pub fn delete_until_eol(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    if let Some((start, end)) = byte_range_until_eol(&doc.rope, &sel) {
        doc.apply(&Transaction::change(&doc.rope,
            [(start, end, None)].into_iter()
            ).set_selection(sel)
        );
        doc.modified = true;
        move_cursor_to(None, None, ctx);
    }
}

pub fn change_until_eol(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    delete_until_eol(ctx);
}

pub fn switch_pane_top(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Up);
    hide_search(ctx);
}

pub fn switch_pane_bottom(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Down);
    hide_search(ctx);
}

pub fn switch_pane_left(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Left);
    hide_search(ctx);
}

pub fn switch_pane_right(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Right);
    hide_search(ctx);
}

pub fn search(ctx: &mut Context) {
    ctx.compositor_callbacks.push(Box::new(|comp, cx| {
        cx.editor.search.focused = true;
        cx.editor.search.total_matches = 0;
        cx.editor.search.current_match = 0;
        comp.remove::<Search>();
        let qhistory = cx.editor.search.query_history.clone();
        comp.push(Box::new(Search::new(qhistory)))
    }));
}

pub fn next_search_match(ctx: &mut Context) {
    if ctx.editor.search.query_history.is_empty() {
        ctx.editor.set_error("No search term found");
    } else {
        ctx.compositor_callbacks.push(Box::new(|comp, cx| {
            cx.editor.search.focused = false;
            crate::search::search(cx, false);
            comp.remove::<Search>();
            comp.push(Box::new(Search::with_term(cx.editor.search.query_history.last().unwrap())));
        }));
    }
}

pub fn prev_search_match(ctx: &mut Context) {
    if ctx.editor.search.query_history.is_empty() {
        ctx.editor.set_error("No search term found");
    } else {
        ctx.compositor_callbacks.push(Box::new(|comp, cx| {
            cx.editor.search.focused = false;
            crate::search::search(cx, true);
            comp.remove::<Search>();
            comp.push(Box::new(Search::with_term(cx.editor.search.query_history.last().unwrap())));
        }));
    }
}

pub fn invert_selection(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let sel = doc.selection(pane.id);
    doc.set_selection(pane.id, sel.invert());
}
