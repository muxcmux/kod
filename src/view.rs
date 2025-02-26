use std::ops::Range;

use crop::Rope;

use crate::{compositor::Context, editor::Mode, graphemes::{self, GraphemeCategory}, language::syntax::{Highlight, HighlightEvent}, panes::Pane, selection::Selection, ui::{buffer::Buffer, scroll::Scroll, style::Style, theme::THEME, Position, Rect}};

/// A wrapper around a HighlightIterator
/// that merges the layered highlights to create the final text style
/// and yields the active text style and the byte at which the active
/// style will have to be recomputed.
pub struct StyleIter<H: Iterator<Item = HighlightEvent>> {
    active_highlights: Vec<Highlight>,
    highlight_iter: H,
}

impl<H: Iterator<Item = HighlightEvent>> StyleIter<H> {
    pub fn new(highlight_iter: H) -> Self {
        Self {
            active_highlights: Vec::with_capacity(64),
            highlight_iter
        }
    }
}

impl<H: Iterator<Item = HighlightEvent>> Iterator for StyleIter<H> {
    type Item = (Style, usize);
    fn next(&mut self) -> Option<(Style, usize)> {
        for event in self.highlight_iter.by_ref() {
            match event {
                HighlightEvent::HighlightStart(highlight) => {
                    self.active_highlights.push(highlight)
                }
                HighlightEvent::HighlightEnd => {
                    self.active_highlights.pop();
                }
                HighlightEvent::Source { end, .. } => {
                    let style = self
                        .active_highlights
                        .iter()
                        .fold(THEME.get("text"), |acc, span| {
                            acc.patch(THEME.highlight_style(*span))
                        });
                    return Some((style, end));
                }
            }
        }
        None
    }
}

#[derive(Default, Debug)]
pub struct View {
    pub scroll: Scroll,
}

impl View {
    pub fn render(
        &self,
        pane: &Pane,
        area: &Rect,
        buffer: &mut Buffer,
        ctx: &Context,
    ) {
        let doc = ctx.editor.documents.get(&pane.doc_id).expect("Can't get doc from pane id");
        let sel = doc.selection(pane.id);
        let highlights = doc.syntax_highlights(pane.view.visible_byte_range(&doc.rope, area.height));
        let mut styles = StyleIter::new(highlights);

        let (mut style, mut highlight_until) = styles.next()
            .unwrap_or((THEME.get("text"), usize::MAX));

        // loop through each visible line
        for row in self.scroll.y..self.scroll.y + area.height as usize {
            if row >= doc.rope.line_len() { break }

            let mut offset = doc.rope.byte_of_line(row);
            // at the start of each line we have to check if the byte offset
            // is more than the current highlight_until (accounting for new lines)
            while offset > highlight_until {
                match styles.next() {
                    Some((s, h)) => (style, highlight_until) = (s, h),
                    None => break
                }
            }

            let line = doc.rope.line(row);
            let mut graphemes = line.graphemes();
            // accounts for multi-width graphemes
            let mut skip_next_n_cols = 0;

            // advance the iterator to account for scroll
            let mut advance = 0;
            while advance < self.scroll.x {
                if let Some(g) = graphemes.next() {
                    offset += g.len();
                    advance += graphemes::width(&g);
                    skip_next_n_cols = advance.saturating_sub(self.scroll.x);
                } else {
                    break
                }
            }

            let y = row.saturating_sub(self.scroll.y) as u16 + area.top();
            let mut trailing_whitespace = vec![];

            for col in self.scroll.x..self.scroll.x + area.width as usize {
                if skip_next_n_cols > 0 {
                    skip_next_n_cols -= 1;
                    continue;
                }
                match graphemes.next() {
                    None => break,
                    Some(g) => {
                        let width = graphemes::width(&g);
                        let x = col.saturating_sub(self.scroll.x) as u16 + area.left();

                        skip_next_n_cols = width - 1;

                        offset += g.len();

                        while offset > highlight_until {
                            match styles.next() {
                                Some((s, h)) => (style, highlight_until) = (s, h),
                                None => break
                            }
                        }

                        buffer.put_symbol(&g, x, y, visual_selection_style(style, sel, col, row, &ctx.editor.mode));

                        if GraphemeCategory::from(&g) == GraphemeCategory::Whitespace {
                            trailing_whitespace.push(x);
                        } else {
                            trailing_whitespace.drain(..);
                        }
                    }
                }
            }

            for x in trailing_whitespace {
                // render trailing whitespace
                buffer.put_symbol("~", x, y, THEME.get("text.whitespace"));
            }
        }

        // render cursors
        if pane.id == ctx.editor.panes.focus {
            for range in sel.ranges.iter() {
                if range != sel.primary() {
                    // Skip rendering non-primary cursors which are off-screen
                    if range.head.x < self.scroll.x ||
                        range.head.y < self.scroll.y ||
                        range.head.x >= self.scroll.x + area.width as usize ||
                        range.head.y >= self.scroll.y + area.height as usize {
                        continue
                    }
                    let position = Position {
                        col: area.left() + (range.head.x - self.scroll.x) as u16,
                        row: area.top() + (range.head.y - self.scroll.y) as u16,
                    };
                    if let Some(style) = buffer.cell_style(position.col, position.row) {
                        let style = match ctx.editor.mode {
                            Mode::Normal => style.patch(THEME.get("ui.multicursor.normal")),
                            Mode::Insert => style.patch(THEME.get("ui.multicursor.insert")),
                            Mode::Replace => style.patch(THEME.get("ui.multicursor.replace")),
                            Mode::Select => style.patch(THEME.get("ui.multicursor.select")),
                        };
                        buffer.set_style(Rect { position, width: 1, height: 1 }, style);
                    }
                }
            }
        }

        // render scrollbar
        let total = doc.rope.line_len();
        let visible = area.height as usize;
        if visible / total == 0 {
            let window = ((visible as f64 / total as f64) * visible as f64) as usize;
            let window = window.max(1);

            let offset = self.scroll.y * visible / total;
            let scroll = area.clip_left(area.width.saturating_sub(1))
                .clip_top(offset as u16)
                .clip_bottom(area.height.saturating_sub(offset as u16 + window as u16));
            buffer.fill_with("▋", THEME.get("ui.scrollbar"), scroll);
        }
    }

    pub fn visible_byte_range(&self, rope: &Rope, height: u16) -> Range<usize> {
        let from = self.scroll.y;
        let to = (from + height.saturating_sub(1) as usize).min(rope.line_len().saturating_sub(1));
        let start = rope.byte_of_line(from);
        let end = rope.byte_of_line(to + 1);

        start..end
    }
}

fn visual_selection_style(
    style: Style,
    sel: &Selection,
    x: usize,
    y: usize,
    mode: &Mode,
) -> Style {
    if mode != &Mode::Select {
        return style
    }

    if sel.ranges.iter().any(|r| r.contains_cursor(x, y)) {
        return style.patch(THEME.get("selection"))
    }

    style
}
