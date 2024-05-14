use std::{borrow::Cow, env, fs, path::PathBuf};

use anyhow::Result;
use crop::Rope;

use crate::document::Document;

#[derive(Eq, Hash, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Insert,
}

pub enum Severity {
    Hint,
    Info,
    Warning,
    Error,
}

pub struct EditorStatus {
    pub severity: Severity,
    pub message: Cow<'static, str>
}

pub struct Editor {
    pub mode: Mode,
    pub document: Document,
    pub quit: bool,
    pub status: Option<EditorStatus>,
}

const SIZE_SUFFIX: [&str; 9] = ["B", "K", "M", "G", "T", "P", "E", "Z", "Y"];
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
    pub fn new() -> Result<Self> {
        let mut args: Vec<String> = env::args().collect();

        let mut path = None;
        let data = if args.len() > 1 {
            let pa = PathBuf::from(args.pop().unwrap());
            let contents = std::fs::read_to_string(&pa)?;
            path = Some(pa);
            Rope::from(if contents.is_empty() { "\n" } else { &contents })
        } else {
            Rope::from("\n")
        };

        let document = Document::new(data, path);

        Ok(Self {
            document,
            mode: Mode::Normal,
            quit: false,
            status: None,
        })
    }

    pub fn save_document(&mut self) {
        if let Some(path) = &self.document.path {
            fs::write(path, self.document.data.to_string()).expect("couldn't write to file");
            let size = format_size_units(self.document.data.byte_len());
            self.set_status(format!("{} written {}", path.to_string_lossy(), size));
            self.document.modified = false;
        } else {
            self.set_error("Can't write a buffer without a path");
        }
    }

    pub fn set_error(&mut self, message: impl Into<Cow<'static, str>>) {
        self.status = Some(EditorStatus { message: message.into(), severity: Severity::Error });
    }

    pub fn set_status(&mut self, message: String) {
        self.status = Some(EditorStatus { message: message.into(), severity: Severity::Info });
    }
}

