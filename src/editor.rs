use crate::{application::Event, commands::actions::GotoCharacterMove, document::DocumentId, graphemes::NEW_LINE, panes::{PaneId, Panes}, registers::Registers, search::SearchState, ui::Rect};
use std::{borrow::Cow, collections::BTreeMap, fs, path::Path, sync::mpsc::{self, Receiver, Sender}, thread, time::{Duration, Instant}};

use crop::Rope;
use smartstring::{LazyCompact, SmartString};

use crate::document::Document;
use anyhow::Result;

#[derive(Eq, Hash, PartialEq, Debug, Clone)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
    Select,
}

#[allow(unused)]
pub enum Severity {
    Hint,
    Info,
    Warning,
    Error,
}

pub struct EditorStatus {
    pub severity: Severity,
    pub message: Cow<'static, str>,
}

struct InputDebounceBuffer {
    timeout: Duration,
    last_invoked: Instant,
    tx: Sender<(char, Sender<Event>)>,
    debounce_tx: Sender<char>,
}

impl InputDebounceBuffer {
    fn new(timeout: Duration) -> Self {
        let (tx, rx) = mpsc::channel::<(char, Sender<Event>)>();
        let (debounce_tx, debounce_rx) = mpsc::channel();
        let mut buffer: SmartString<LazyCompact> = SmartString::new();

        thread::spawn(move || {
            while let Ok((c, reply)) = rx.recv() {
                buffer.push(c);
                while let Ok(c) = debounce_rx.recv_timeout(timeout) {
                    buffer.push(c);
                }
                _ = reply.send(Event::BufferedInput(buffer.clone()));
                buffer.clear();
            }
        });

        InputDebounceBuffer {
            timeout,
            last_invoked: Instant::now(),
            tx,
            debounce_tx,
        }
    }

    fn buffer(&mut self, c: char, reply: Sender<Event>) {
        if self.last_invoked.elapsed() > self.timeout {
            _ = self.tx.send((c, reply));
        } else {
            _ = self.debounce_tx.send(c);
        }

        self.last_invoked = Instant::now();
    }
}


pub struct Editor {
    pub mode: Mode,
    pub panes: Panes,
    pub registers: Registers,
    pub search: SearchState,
    pub documents: BTreeMap<DocumentId, Document>,
    next_doc_id: DocumentId,
    pub status: Option<EditorStatus>,
    pub last_goto_character_move: Option<GotoCharacterMove>,
    input_buffer: InputDebounceBuffer,
    pub tx: Sender<Event>,
    pub rx: Receiver<Event>,
}

const SIZE_SUFFIX: [&str; 9] = ["b", "kb", "mb", "gb", "tb", "there is", "a special place", "in hell", "for you"];
const SIZE_UNIT: f64 = 1024.0;

fn format_size_units(bytes: usize) -> String {
    let bytes = bytes as f64;
    let base = bytes.log10() / SIZE_UNIT.log10();
    let size = SIZE_UNIT.powf(base - base.floor());
    let value = format!("{:.1}", size);
    let value = value.trim_end_matches(".0");
    [value, SIZE_SUFFIX[base.floor() as usize]].join("")
}

impl Editor {
    pub fn new(area: Rect) -> Self {
        // Remove 1 from bottom for status line
        let panes = Panes::new(area.clip_bottom(1));

        let (tx, rx) = mpsc::channel();

        Self {
            mode: Mode::Normal,
            next_doc_id: DocumentId::default(),
            documents: BTreeMap::new(),
            status: None,
            panes,
            rx,
            tx,
            last_goto_character_move: None,
            registers: Registers::default(),
            input_buffer: InputDebounceBuffer::new(Duration::from_millis(10)),
            search: SearchState::default(),
        }
    }

    pub fn save_document(&mut self, doc_id: DocumentId) {
        let doc = self.documents.get_mut(&doc_id).unwrap();
        if doc.readonly {
            self.set_error("Cannot save a Readonly document");
        } else if let Some(path) = &doc.path {
            match fs::write(path, doc.rope.to_string()) {
                Ok(_) => {
                    let size = format_size_units(doc.rope.byte_len());
                    let lines = doc.rope.line_len();
                    doc.save();
                    self.set_status(format!("{} lines written ({})", lines, size));
                },
                Err(err) => {
                    self.set_error(format!("{err}"));
                },
            }
        } else {
            self.set_error("Don't know where to save to");
        }
    }

    pub fn has_unsaved_docs(&self) -> bool {
        self.documents.iter().any(|(_, doc)| doc.is_modified())
    }

    pub fn open(&mut self, pane_id: PaneId, path: &Path) -> Result<(bool, DocumentId)> {
        let id = self.documents.values()
            .find(|doc| {
                match &doc.path {
                    Some(p) => p == path,
                    None => false,
                }
            })
            .map(|doc| doc.id);

        // hard_wrapped is used only to indicate that a document
        // was modified when it was open. We set this to false
        // for existing documents, because if they were wrapped
        // when opened for the first time, the notification was
        // already displayed to the user once
        let (hard_wrapped, id) = if let Some(id) = id {
            (false, id)
        } else {
            let next_id = self.next_doc_id;
            let (hard_wrapped, doc) = Document::open(next_id, pane_id, path)?;

            self.documents.insert(next_id, doc);
            self.next_doc_id.advance();
            (hard_wrapped, next_id)
        };

        Ok((hard_wrapped, id))
    }

    pub fn open_scratch(&mut self, pane_id: PaneId) -> DocumentId {
        let rope = Rope::from(NEW_LINE.to_string());
        let next_id = self.next_doc_id;
        let doc = Document::new(next_id, pane_id, rope, None);
        self.documents.insert(next_id, doc);
        self.next_doc_id.advance();
        next_id
    }

    pub fn set_error(&mut self, message: impl Into<Cow<'static, str>>) {
        self.status = Some(EditorStatus {
            message: message.into(),
            severity: Severity::Error,
        });
    }

    pub fn set_warning(&mut self, message: impl Into<Cow<'static, str>>) {
        self.status = Some(EditorStatus {
            message: message.into(),
            severity: Severity::Warning,
        });
    }

    pub fn set_status(&mut self, message: impl Into<Cow<'static, str>>) {
        self.status = Some(EditorStatus {
            message: message.into(),
            severity: Severity::Info,
        });
    }

    pub fn quit(&self) {
        _ = self.tx.send(Event::Quit);
    }

    pub fn request_buffered_input(&mut self, c: char) {
        self.input_buffer.buffer(c, self.tx.clone());
    }
}
