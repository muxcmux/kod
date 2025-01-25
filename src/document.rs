use std::{borrow::Cow, cell::Cell, collections::HashMap, path::{Path, PathBuf}, sync::Arc};

use crop::Rope;
use crate::{graphemes::NEW_LINE, history::{History, State, Transaction}, language::{syntax::{HighlightEvent, Syntax}, LanguageConfiguration, LANG_CONFIG}, panes::PaneId, selection::Selection};

use anyhow::{bail, Result};

make_inc_id_type!(DocumentId);

static SCRATCH: &str = "[scratch]";

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
            selections: HashMap::new(),
            last_saved_revision: 0,
        }
    }

    pub fn open(id: DocumentId, path: &Path) -> Result<Self> {
        if !path.metadata()?.is_file() {
            bail!("Cannot open path: {:?}", path)
        }

        let mut contents = std::fs::read_to_string(path)?;

        if contents.is_empty() {
            contents = NEW_LINE.to_string();
        }

        let rope = Rope::from(contents);

        Ok(Self::new(id, rope, Some(path.to_path_buf())))
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

    pub fn selection(&self, pane_id: PaneId) -> Selection {
        if let Some(s) = self.selections.get(&pane_id) {
            return *s;
        }

        Selection::default()
    }

    // pub fn selections(&self) -> &HashMap<PaneId, Selection> {
    //     &self.selections
    // }

    pub fn set_selection(&mut self, pane_id: PaneId, selection: Selection) {
        self.selections.insert(pane_id, selection);
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
                selection: transaction.selection,
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
            ret = Some(t.selection);
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
