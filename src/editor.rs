use std::{borrow::Cow, env, fs, path::PathBuf};

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
    pub message: Cow<'static, str>,
}

pub struct Editor {
    pub mode: Mode,
    pub document: Document,
    pub quit: bool,
    pub status: Option<EditorStatus>,
}

const SIZE_SUFFIX: [&str; 9] = ["b", "k", "m", "g", "t", "p", "e", "z", "y"];
const SIZE_UNIT: f64 = 1024.0;

fn format_size_units(bytes: usize) -> String {
    let bytes = bytes as f64;
    let base = bytes.log10() / SIZE_UNIT.log10();
    let size = SIZE_UNIT.powf(base - base.floor());
    let value = format!("{:.1}", size);
    let value = value.trim_end_matches(".0");
    [value, SIZE_SUFFIX[base.floor() as usize]].join("")
}

impl Default for Editor {
    fn default() -> Self {
        let mut args: Vec<String> = env::args().collect();

        let mut path = None;
        let mut status = None;
        let mut contents = "\n".to_string();

        if args.len() > 1 {
            let pa = PathBuf::from(args.pop().unwrap());
            if pa.is_file() {
                match std::fs::read_to_string(&pa) {
                    Ok(c) => {
                        if !c.is_empty() { contents = c; }
                    },
                    Err(err) => {
                        status = Some(EditorStatus { severity: Severity::Error, message: format!("{err}").into() })
                    },
                }
            }
            path = Some(pa);
        }

        let document = Document::new(Rope::from(contents), path);

        Self {
            document,
            status,
            mode: Mode::Normal,
            quit: false,
        }
    }
}
impl Editor {
    pub fn save_document(&mut self) {
        if let Some(path) = &self.document.path {
            match fs::write(path, self.document.data.to_string()) {
                Ok(_) => {
                    let size = format_size_units(self.document.data.byte_len());
                    self.set_status(format!("{} written to {}", size, path.to_string_lossy()));
                    self.document.modified = false;
                },
                Err(err) => {
                    self.set_error(format!("{err}"));
                },
            }
        } else {
            self.set_error("Don't know where to save to");
        }
    }

    pub fn set_error(&mut self, message: impl Into<Cow<'static, str>>) {
        self.status = Some(EditorStatus {
            message: message.into(),
            severity: Severity::Error,
        });
    }

    pub fn set_status(&mut self, message: String) {
        self.status = Some(EditorStatus {
            message: message.into(),
            severity: Severity::Info,
        });
    }
}
