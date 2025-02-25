/// Mostly copied from helix with the difference that
/// this doesn't have a change set but operates with
/// transactions straight away
use std::{num::NonZeroUsize, ops::Range};

use crop::Rope;
use smartstring::{LazyCompact, SmartString};
use std::cmp::Ordering;
use crate::selection::Selection;

pub struct State {
    pub rope: Rope,
    pub selection: Selection,
}

/// Range of start_byte..end_byte and the replacement string (None to delete)
pub type Change = (Range<usize>, Option<SmartString<LazyCompact>>);

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Operation {
    // keep n bytes
    Retain(usize),
    // delete n bytes
    Delete(usize),
    // insert string
    Insert(SmartString<LazyCompact>),
}

use Operation::*;

pub struct History {
    revisions: Vec<Revision>,
    pub current: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            current: 0,
            revisions: vec![Revision {
                parent: 0,
                last_child: None,
                // timestamp: Instant::now(),
                transaction: Transaction::default(),
                inversion: Transaction::default(),
            }]
        }
    }
}

impl History {
    pub fn commit_revision(&mut self, transaction: Transaction, original: &State) {
        let inversion = transaction.invert(original);
        let new_current = self.revisions.len();
        // let timestamp = Instant::now();

        self.revisions[self.current].last_child = NonZeroUsize::new(new_current);

        self.revisions.push(Revision {
            parent: self.current,
            last_child: None,
            transaction,
            inversion,
            // timestamp,
        });

        self.current = new_current;
    }

    pub fn undo(&mut self) -> Option<&Transaction> {
        if self.current == 0 {
            return None;
        }

        let current_revision = &self.revisions[self.current];
        self.current = current_revision.parent;
        Some(&current_revision.inversion)
    }

    pub fn redo(&mut self) -> Option<&Transaction> {
        let current_revision = &self.revisions[self.current];
        let last_child = current_revision.last_child?;
        self.current = last_child.get();

        Some(&self.revisions[last_child.get()].transaction)
    }
}

struct Revision {
    parent: usize,
    last_child: Option<NonZeroUsize>,
    transaction: Transaction,
    inversion: Transaction,
    // timestamp: Instant,
}

#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct Transaction {
    pub operations: Vec<Operation>,
    pub selection: Selection,
}

impl Transaction {
    pub fn empty() -> Self {
        Self {
            operations: vec![],
            selection: Selection::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    pub fn change<I>(rope: &Rope, changes: I) -> Self
    where
        I: Iterator<Item = Change>
    {
        let mut operations = Vec::with_capacity(changes.size_hint().1.unwrap_or(1));
        let mut last = 0;

        for (Range {start, end, ..}, text) in changes {
            debug_assert!(last <= start, "last({last}) is not <= start({start})");
            debug_assert!(start <= end, "start({start}) is not <= end({end})");

            operations.push(Retain(start - last));

            if let Some(t) = text {
                operations.push(Insert(t));
            }

            operations.push(Delete(end - start));

            last = end
        }

        operations.push(Retain(rope.byte_len() - last));

        Self { operations, ..Default::default() }
    }

    pub fn set_selection(mut self, selection: Selection) -> Self {
        self.selection = selection;
        self
    }

    fn insert(&mut self, text: SmartString<LazyCompact>) {
        if text.is_empty() { return }

        let new_last = match self.operations.as_mut_slice() {
            [.., Insert(prev)] | [.., Insert(prev), Delete(_)] => {
                prev.push_str(&text);
                return;
            }
            [.., last @ Delete(_)] => std::mem::replace(last, Insert(text)),
            _ => Insert(text),
        };

        self.operations.push(new_last);
    }

    fn delete(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        if let Some(Delete(count)) = self.operations.last_mut() {
            *count += n;
        } else {
            self.operations.push(Delete(n));
        }
    }

    fn retain(&mut self, n: usize) {
        if n == 0 {
            return;
        }

        if let Some(Retain(count)) = self.operations.last_mut() {
            *count += n;
        } else {
            self.operations.push(Retain(n));
        }
    }

    pub fn compose(self, other: Self) -> Self {
        if self.operations.is_empty() {
            return other
        }

        if other.operations.is_empty() {
            return self
        }

        let len = self.operations.len();

        let mut operations_a = self.operations.into_iter();
        let mut operations_b = other.operations.into_iter();

        let mut next_a = operations_a.next();
        let mut next_b = operations_b.next();

        let mut transaction = Self {
            operations: Vec::with_capacity(len),
            selection: other.selection,
        };

        loop {
            match (next_a, next_b) {
                (None, None) => { break; },
                (Some(Delete(n)), b) => {
                    transaction.delete(n);
                    next_a = operations_a.next();
                    next_b = b;
                },
                (a, Some(Insert(text))) => {
                    transaction.insert(text);
                    next_a = a;
                    next_b = operations_b.next();

                },
                (None, val) | (val, None) => unreachable!("({:?})", val),
                (Some(Retain(i)), Some(Retain(j))) => match i.cmp(&j) {
                    Ordering::Less => {
                        transaction.retain(i);
                        next_a = operations_a.next();
                        next_b = Some(Retain(j - i));
                    }
                    Ordering::Equal => {
                        transaction.retain(i);
                        next_a = operations_a.next();
                        next_b = operations_b.next();
                    }
                    Ordering::Greater => {
                        transaction.retain(j);
                        next_a = Some(Retain(i - j));
                        next_b = operations_b.next();
                    }
                },
                (Some(Insert(mut s)), Some(Delete(j))) => {
                    let len = s.bytes().count();

                    match len.cmp(&j) {
                        Ordering::Less => {
                            next_a = operations_a.next();
                            next_b = Some(Delete(j - len));
                        }
                        Ordering::Equal => {
                            next_a = operations_a.next();
                            next_b = operations_b.next();
                        }
                        Ordering::Greater => {
                            let (pos, _) = s.bytes().enumerate().nth(j).unwrap();
                            s.replace_range(0..pos, "");
                            next_a = Some(Insert(s));
                            next_b = operations_b.next();
                        }
                    }
                },
                (Some(Insert(s)), Some(Retain(j))) => {
                    let len = s.bytes().count();
                    match len.cmp(&j) {
                        Ordering::Less => {
                            transaction.insert(s);
                            next_a = operations_a.next();
                            next_b = Some(Retain(j - len));
                        }
                        Ordering::Equal => {
                            transaction.insert(s);
                            next_a = operations_a.next();
                            next_b = operations_b.next();
                        }
                        Ordering::Greater => {
                            let (pos, _) = s.bytes().enumerate().nth(j).unwrap();
                            let mut before = s;
                            let after = before.split_off(pos);

                            transaction.insert(before);
                            next_a = Some(Insert(after));
                            next_b = operations_b.next();
                        }
                    }
                },
                (Some(Retain(i)), Some(Delete(j))) => match i.cmp(&j) {
                    Ordering::Less => {
                        transaction.delete(i);
                        next_a = operations_a.next();
                        next_b = Some(Delete(j - i));
                    }
                    Ordering::Equal => {
                        transaction.delete(j);
                        next_a = operations_a.next();
                        next_b = operations_b.next();
                    }
                    Ordering::Greater => {
                        transaction.delete(j);
                        next_a = Some(Retain(i - j));
                        next_b = operations_b.next();
                    }
                },
            }
        }

        transaction
    }

    pub fn invert(&self, original: &State) -> Self {
        let mut transaction = Self {
            operations: Vec::with_capacity(self.operations.len()),
            selection: original.selection.clone(),
        };

        let mut offset = 0;

        for operation in &self.operations {
            match operation {
                Retain(n) => {
                    transaction.retain(*n);
                    offset += n;
                }
                Delete(n) => {
                    let text = original.rope.byte_slice(offset..offset + n);
                    transaction.insert(text.to_string().into());
                    offset += n;
                }
                Insert(s) => {
                    let bytes = s.bytes().count();
                    transaction.delete(bytes);
                }
            }
        }

        transaction
    }

    pub fn apply(&self, rope: &mut Rope) {
        let mut cursor = 0;

        for op in &self.operations {
            match op {
                Retain(bytes) => { cursor += bytes; },
                Delete(bytes) => { rope.delete(cursor..cursor + *bytes) },
                Insert(text) => {
                    rope.insert(cursor, text);
                    cursor += text.len();
                },
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crop::Rope;
    use smallvec::SmallVec;
    use crate::history::State;
    use crate::selection;

    use super::Transaction;
    use super::Operation::*;

    #[test]
    fn transaction_change() {
        let mut rope = Rope::from("hello world!\ntest world bar");

        let transaction = Transaction::change(
            &rope,
            [
                (6..11, Some("foo".into())),
                (12..17, None),
                (18..23, Some("foo".into())),
                (27..27, Some("!".into())),
            ].into_iter(),
        );
        transaction.apply(&mut rope);

        assert!(rope == "hello foo! foo bar!", "ROPE: '{rope}'");
    }

    #[test]
    fn transaction_composition() {
        let selection = selection::Selection {
            primary_index: 0,
            ranges: SmallVec::from(
                [selection::Range { head: selection::Cursor { x: 1, y: 0 }, ..Default::default() }]
            )
        };
        let a = Transaction {
            selection,
            operations: vec![
                Retain(5),
                Insert(" test!".into()),
                Retain(1),
                Delete(2),
                Insert("abc".into()),
            ],
        };

        let selection = selection::Selection {
            primary_index: 0,
            ranges: SmallVec::from(
                [selection::Range { head: selection::Cursor { x: 5, y: 0 }, ..Default::default() }]
            )
        };
        let b = Transaction {
            selection,
            operations: vec![
                Delete(10),
                Insert("世orld".into()),
                Retain(5),
            ],
        };

        let mut text = Rope::from("hello xz");

        let composed = a.compose(b);
        composed.apply(&mut text);
        assert_eq!(text, "世orld! abc");
        assert_eq!(composed.selection.primary().head.x, 5);
        assert_eq!(composed.selection.primary().head.y, 0);
    }

    #[test]
    fn transaction_invert() {
        let transaction = Transaction {
            selection: selection::Selection::default(),
            operations: vec![
                Retain(3),
                Insert("test".into()),
                Delete(5),
                Retain(3)
            ],
        };

        let doc = Rope::from("世界3 hello xz");

        let mut doc2 = doc.clone();

        let state = State {
            rope: doc.clone(),
            selection: selection::Selection::default(),
        };

        let revert = transaction.invert(&state);


        transaction.apply(&mut doc2);

        assert_ne!(transaction, revert);
        assert_ne!(doc, doc2);

        let state2 = State {
            rope: doc2.clone(),
            selection: selection::Selection::default(),
        };

        assert_eq!(transaction, revert.invert(&state2));

        revert.apply(&mut doc2);
        assert_eq!(doc, doc2);
    }
}
