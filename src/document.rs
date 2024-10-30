use std::{borrow::Cow, cell::Cell, path::PathBuf};

use crop::Rope;
use crate::history::{History, State, Transaction};

make_inc_id_type!(DocumentId);

pub struct Document {
    pub id: DocumentId,
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub readonly: bool,
    history: Cell<History>,
    transaction: Cell<Transaction>,
    old_state: Option<State>
}

impl Document {
    pub fn new(id: DocumentId, rope: Rope, path: Option<PathBuf>) -> Self {
        let readonly = match &path {
            Some(p) => {
                std::fs::metadata(p).is_ok_and(|m| {
                    m.permissions().readonly()
                })
            },
            None => false,
        };
        Self {
            id,
            rope,
            transaction: Cell::new(Transaction::default()),
            history: Cell::new(History::default()),
            old_state: None,
            path,
            readonly,
            modified: false,
        }
    }

    pub fn filename(&self) -> Cow<'_, str> {
        match &self.path {
            Some(p) => match p.file_name() {
                Some(f) => f.to_string_lossy(),
                None => "[scratch]".into()
            }
            None => "[scratch]".into(),
        }
    }

    pub fn apply(&mut self, transaction: &Transaction) {
        if transaction.is_empty() {
            return
        }

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
}
