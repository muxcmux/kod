use std::time::SystemTime;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::cell::Cell;
use std::borrow::Cow;

use crop::Rope;
use crate::selection::Selection;
use crate::panes::PaneId;
use crate::language::{syntax::{HighlightEvent, Syntax}, LanguageConfiguration, LANG_CONFIG};
use crate::history::{Change, History, State, Transaction};
use crate::graphemes::NEW_LINE_STR;

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
    pub last_modified_at: SystemTime,
    last_saved_revision: usize,
    selections: HashMap<PaneId, Selection>,
    history: Cell<History>,
    transaction: Cell<Transaction>,
    old_state: Option<State>
}

impl Document {
    pub fn new(id: DocumentId, pane_id: PaneId) -> Self {
        Self {
            id,
            rope: Rope::from(NEW_LINE_STR),
            language: None,
            syntax: None,
            transaction: Cell::new(Transaction::default()),
            history: Cell::new(History::default()),
            old_state: None,
            path: None,
            readonly: false,
            selections: HashMap::from([(pane_id, Selection::default())]),
            last_modified_at: SystemTime::now(),
            last_saved_revision: 0,
        }
    }

    fn load_from_path(&mut self) -> Result<bool> {
        if self.path.is_none() {
            bail!("Cannot load contents for a document without a path")
        }

        let path = self.path.as_ref().unwrap();

        if !path.exists() {
            bail!("Path {:?} no longer exists", path)
        }

        if !path.metadata()?.is_file() {
            bail!("Cannot load path: {:?}", self.path)
        }

        if let Ok(md) = path.metadata() {
            if let Ok(t) = md.modified() {
                self.last_modified_at = t;
            }
        }

        let contents = std::fs::read_to_string(path)?;

        self.rope = if contents.is_empty() {
            Rope::from(NEW_LINE_STR)
        } else {
            Rope::from(contents)
        };

        self.readonly = path.metadata().is_ok_and(|m| m.permissions().readonly());
        self.language = LANG_CONFIG.language_config_for_path(path)
                            .or(LANG_CONFIG.language_config_for_shebang(self.rope.line(0)));

        if let Some(lang) = &self.language {
            if let Some(config) = lang.highlight_config() {
                self.syntax = Syntax::new(self.rope.clone(), config);
            }
        }

        Ok(self.hard_wrap_long_lines())
    }

    pub fn open(id: DocumentId, pane_id: PaneId, path: &Path) -> Result<(bool, Self)> {
        let mut doc = Self::new(id, pane_id);
        doc.path = Some(path.to_path_buf());
        Ok((doc.load_from_path()?, doc))
    }

    pub fn reload(&mut self) -> Result<bool> {
        let hard_wrapped = self.load_from_path()?;

        // TODO: handle transaction stuff otherwise we crash
        log::warn!("reloaded doc without transaction. undo/redo might cause a panic");
        self.save();

        Ok(hard_wrapped)
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

    // Checks if the document has been modified by us
    pub fn is_modified(&self) -> bool {
        let history = self.history.take();
        let transaction = self.transaction.take();
        let current_revision = history.current;
        let transaction_is_empty = transaction.is_empty();
        self.history.set(history);
        self.transaction.set(transaction);
        current_revision != self.last_saved_revision || !transaction_is_empty
    }

    // Checks if the file of the document has been modified externally
    // NOTE: This will not check if the file still exists on disk and will
    // return false (not changed) in this case
    pub fn was_changed(&self) -> bool {
        match self.path.as_ref() {
            None => false,
            Some(path) => {
                if let Ok(md) = path.metadata() {
                    if let Ok(t) = md.modified() {
                        return t > self.last_modified_at
                    }
                }

                false
            }
        }
    }

    fn get_current_revision(&mut self) -> usize {
        let history = self.history.take();
        let current_revision = history.current;
        self.history.set(history);
        current_revision
    }

    pub fn save(&mut self) {
        self.last_saved_revision = self.get_current_revision();
        if let Some(path) = &self.path {
            self.last_modified_at = path.metadata()
                .map(|m| m.modified().unwrap_or(SystemTime::now()))
                .unwrap_or(SystemTime::now())
        }
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
