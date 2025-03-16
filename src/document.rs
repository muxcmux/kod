use std::{borrow::Cow, cell::Cell, collections::HashMap, path::{Path, PathBuf}, sync::Arc};

use crop::Rope;
use crate::{graphemes::{NEW_LINE, NEW_LINE_STR}, history::{Change, History, State, Transaction}, language::{syntax::{HighlightEvent, Syntax}, LanguageConfiguration, LANG_CONFIG}, panes::PaneId, selection::Selection};

use anyhow::{bail, Result};

make_inc_id_type!(DocumentId);

static SCRATCH: &str = "[scratch]";

pub fn cwd_relative_name(path: &Path) -> Cow<'_, str> {
    match path.file_name() {
        Some(f) => {
            if let Ok(cwd) = std::env::current_dir() {
                if !path.starts_with(&cwd) {
                    return path.to_string_lossy()
                }
                return path.to_string_lossy().replace(&format!("{}/", cwd.to_string_lossy()), "").into()
            }
            f.to_string_lossy()
        },
        None => SCRATCH.into()
    }
}

pub struct Document {
    pub id: DocumentId,
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub readonly: bool,
    pub language: Option<Arc<LanguageConfiguration>>,
    pub syntax: Option<Syntax>,
    last_saved_revision: usize,
    selections: HashMap<PaneId, Selection>,
    history: Cell<History>,
    transaction: Cell<Transaction>,
    old_state: Option<State>
}

impl Document {
    pub fn new(id: DocumentId, pane_id: PaneId, rope: Rope, path: Option<PathBuf>) -> Self {
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
            selections: HashMap::from([(pane_id, Selection::default())]),
            last_saved_revision: 0,
        }
    }

    pub fn open(id: DocumentId, pane_id: PaneId, path: &Path) -> Result<(bool, Self)> {
        if !path.metadata()?.is_file() {
            bail!("Cannot open path: {:?}", path)
        }

        let mut contents = std::fs::read_to_string(path)?;

        if contents.is_empty() {
            contents = NEW_LINE.to_string();
        }

        let rope = Rope::from(contents);

        let mut doc = Self::new(id, pane_id, rope, Some(path.to_path_buf()));
        let hard_wrapped = doc.hard_wrap_long_lines();

        Ok((hard_wrapped, doc))
    }

    // Insert line breaks on lines longer than LIMIT bytes
    // and set the document to Readonly. This is to prevent
    // pathological behaviour with extremely long lines.
    fn hard_wrap_long_lines(&mut self) -> bool {
        const LIMIT: usize = 10_000;
        let mut insert_lines_at = vec![];

        for (i, line) in self.rope.lines().enumerate() {
            let mut offset = LIMIT;
            let len = line.byte_len();

            'outer: while offset < len {
                while !line.is_grapheme_boundary(offset) {
                    offset += 1;
                    if len == offset { continue 'outer }
                }

                insert_lines_at.push(self.rope.byte_of_line(i) + offset + insert_lines_at.len());
                offset += LIMIT;
            }
        }

        let wrap_result = !insert_lines_at.is_empty();
        self.readonly = wrap_result;

        for offset in insert_lines_at {
            self.apply(
                &Transaction::change(
                    &self.rope,
                    [(offset..offset, Some(NEW_LINE_STR.into()))].into_iter()
                )
            );
        }

        wrap_result
    }

    pub fn is_modified(&self) -> bool {
        let history = self.history.take();
        let transaction = self.transaction.take();
        let current_revision = history.current;
        let transaction_is_empty = transaction.is_empty();
        self.history.set(history);
        self.transaction.set(transaction);
        current_revision != self.last_saved_revision || !transaction_is_empty
    }

    fn get_current_revision(&mut self) -> usize {
        let history = self.history.take();
        let current_revision = history.current;
        self.history.set(history);
        current_revision
    }

    pub fn save(&mut self) {
        self.last_saved_revision = self.get_current_revision();
    }

    pub fn filename_display(&self) -> Cow<'_, str> {
        match &self.path {
            Some(p) => cwd_relative_name(p),
            None => SCRATCH.into(),
        }
    }

    pub fn selection(&self, pane_id: PaneId) -> &Selection {
        &self.selections[&pane_id]
    }

    pub fn set_selection(&mut self, pane_id: PaneId, selection: Selection) {
        self.selections.insert(pane_id, selection);
    }

    pub fn modify(&mut self, changes: Vec<Change>, sel: Selection) -> Option<Transaction> {
        if self.readonly {
            return None
        }

        let transaction = Transaction::change(&self.rope, changes.into_iter()).set_selection(sel);
        self.apply(&transaction);
        Some(transaction)
    }

    fn apply(&mut self, transaction: &Transaction) {
        if transaction.is_empty() {
            return
        }

        let old_doc = self.rope.clone();

        let t = self.transaction.take();

        if t.is_empty() {
            self.old_state = Some(State {
                rope: self.rope.clone(),
                selection: transaction.selection.clone(),
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

    pub fn undo_redo(&mut self, undo: bool) -> Option<Selection> {
        let mut history = self.history.take();

        let mut ret = None;

        if let Some(t) = if undo { history.undo() } else { history.redo() } {
            self.apply(t);
            ret = Some(t.selection.clone());
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
                    // start: range.start,
                    end: range.end,
                }]
                .into_iter(),
            ),
        }
    }
}
