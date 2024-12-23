use crate::{application::Event, document::DocumentId, graphemes::NEW_LINE, panes::Panes, registers::Registers, search::SearchState, ui::Rect};
use std::{borrow::Cow, collections::BTreeMap, env, fs, path::PathBuf, sync::mpsc::{self, Receiver, Sender}};

use crop::Rope;

use crate::document::Document;

#[derive(Eq, Hash, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Replace,
}

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

pub struct Editor {
    pub mode: Mode,
    pub panes: Panes,
    pub registers: Registers,
    pub search: SearchState,
    pub documents: BTreeMap<DocumentId, Document>,
    //next_doc_id: DocumentId,
    pub status: Option<EditorStatus>,
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
        let mut args: Vec<String> = env::args().collect();

        let mut path = None;
        let mut status = None;
        let mut contents = NEW_LINE.to_string();

        if args.len() > 1 {
            let pa = PathBuf::from(args.pop().unwrap());
            if pa.is_file() {
                match std::fs::read_to_string(&pa) {
                    Ok(c) => {
                        if !c.is_empty() { contents = c; }
                        path = pa.canonicalize().ok();
                    },
                    Err(err) => {
                        status = Some(EditorStatus { severity: Severity::Error, message: format!("{err}").into() })
                    },
                }
            }
        }

        let doc_id = DocumentId::default();
        let doc = Document::new(doc_id, Rope::from(contents), path);
        let mut documents = BTreeMap::new();
        documents.insert(doc_id, doc);

        // Remove 1 from bottom for status line
        let panes = Panes::new(area.clip_bottom(1));

        let (tx, rx) = mpsc::channel();

        Self {
            mode: Mode::Normal,
            //next_doc_id: doc_id.next(),
            documents,
            status,
            panes,
            rx,
            tx,
            registers: Registers::default(),
            search: SearchState::default(),
        }
    }

    pub fn save_document(&mut self, doc_id: DocumentId) {
        let doc = self.documents.get_mut(&doc_id).unwrap();
        if let Some(path) = &doc.path {
            match fs::write(path, doc.rope.to_string()) {
                Ok(_) => {
                    let size = format_size_units(doc.rope.byte_len());
                    let lines = doc.rope.line_len();
                    doc.modified = false;
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
        self.documents.iter().any(|(_, doc)| doc.modified)
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
}
