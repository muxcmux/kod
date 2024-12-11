use std::{borrow::Cow, cell::Cell, path::PathBuf, sync::Arc};

use crop::Rope;
use crate::{history::{History, State, Transaction}, language::syntax::{Highlight, HighlightEvent, LanguageConfiguration, Syntax, LANG_CONFIG}, ui::{style::Style, theme::THEME}};

make_inc_id_type!(DocumentId);

static SCRATCH: &str = "[scratch]";

pub struct Document {
    pub id: DocumentId,
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub readonly: bool,
    pub language: Option<Arc<LanguageConfiguration>>,
    pub syntax: Option<Syntax>,
    history: Cell<History>,
    transaction: Cell<Transaction>,
    old_state: Option<State>
}

impl Document {
    pub fn new(id: DocumentId, rope: Rope, path: Option<PathBuf>) -> Self {
        let (language, readonly) = match &path {
            Some(p) => {
                let ro = std::fs::metadata(p).is_ok_and(|m| m.permissions().readonly());
                let lc = LANG_CONFIG.language_config_for_path(p)
                        .or(LANG_CONFIG.language_config_for_shebang(rope.line(0)));
                (lc, ro)
            },
            None => (None, false)
        };

        let syntax = match language {
            Some(ref lang) => match lang.highlight_config() {
                Some(cfg) => Syntax::new(rope.clone(), cfg),
                None => None
            }
            None => None
        };

        Self {
            id,
            rope,
            language,
            syntax,
            transaction: Cell::new(Transaction::default()),
            history: Cell::new(History::default()),
            old_state: None,
            path,
            readonly,
            modified: false,
        }
    }

    pub fn filename_display(&self) -> Cow<'_, str> {
        match &self.path {
            Some(p) => match p.file_name() {
                Some(f) => {
                    if let Ok(cwd) = std::env::current_dir() {
                        if !p.starts_with(cwd) {
                            return p.to_string_lossy()
                        }
                    }
                    f.to_string_lossy()
                },
                None => SCRATCH.into()
            }
            None => SCRATCH.into(),
        }
    }

    pub fn apply(&mut self, transaction: &Transaction) {
        if transaction.is_empty() {
            return
        }

        let old_doc = self.rope.clone();

        let t = self.transaction.take();

        if t.is_empty() {
            self.old_state = Some(State {
                rope: self.rope.clone(),
                cursor_x: transaction.cursor_x,
                cursor_y: transaction.cursor_y,
            });
        }

        transaction.apply(&mut self.rope);

        // Compose this transaction with the previous one
        self.transaction.set(t.compose(transaction.clone()));

        if let Some(syntax) = &mut self.syntax {
            let res = syntax.update(
                old_doc,
                self.rope.clone(),
                transaction,
            );
            if res.is_err() {
                log::error!("TS parser failed, disabling TS for the current buffer: {res:?}");
                self.syntax = None;
            }
        }
    }

    pub fn commit_transaction_to_history(&mut self) {
        let t = self.transaction.take();

        if t.is_empty() {
            return;
        }

        let old_state = self.old_state.take().expect("no old_state available");

        let mut history = self.history.take();
        history.commit_revision(t, &old_state);
        self.history.set(history);
    }

    pub fn undo_redo(&mut self, undo: bool) -> Option<(usize, usize)> {
        let mut history = self.history.take();

        let mut ret = None;

        if let Some(t) = if undo { history.undo() } else { history.redo() } {
            self.apply(t);
            ret = Some((t.cursor_x, t.cursor_y));
        }

        self.history.set(history);
        self.transaction.take();

        ret
    }

    pub fn syntax_highlights<'doc>(
        &'doc self,
        range: std::ops::Range<usize>,
    ) -> Box<dyn Iterator<Item = HighlightEvent> + 'doc> {
        match self.syntax {
            Some(ref syntax) => {
                let iter = syntax
                    // TODO: range doesn't actually restrict source, just highlight range
                    .highlight_iter(self.rope.byte_slice(..), Some(range), None)
                    .map(|event| event.unwrap());

                Box::new(iter)
            }
            None => Box::new(
                [HighlightEvent::Source {
                    start: range.start,
                    end: range.end,
                }]
                .into_iter(),
            ),
        }
    }
}

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

