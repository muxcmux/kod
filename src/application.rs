use std::thread;
use std::sync::mpsc::{self, Sender};
use std::path::PathBuf;
use std::env;
use std::time::Duration;

use crossterm::{cursor::SetCursorStyle, event::{read, KeyEvent, KeyEventKind}};
use notify_debouncer_full::{new_debouncer, notify::RecursiveMode, DebouncedEvent};
use smartstring::{LazyCompact, SmartString};
use crate::ui::{terminal::{self, Terminal}, Rect};
use crate::panes::PaneId;
use crate::editor::Editor;
use crate::compositor::{Compositor, Context};
use crate::components::{editor_view::EditorView, files::Files, status_line::StatusLine};
use anyhow::Result;

pub enum Event {
    Draw,
    Quit,
    Term(crossterm::event::Event),
    BufferedInput(SmartString<LazyCompact>),
    FileEvent(DebouncedEvent),
}

pub struct Application {
    editor: Editor,
    compositor: Compositor,
    terminal: Terminal,
}

impl Default for Application {
    fn default() -> Self {
        // Setup
        let size = crossterm::terminal::size().expect("Can't get terminal size");
        let size = Rect::from(size);

        let mut editor = Editor::new(size);
        let terminal = Terminal::new(size);
        let mut compositor = Compositor::new(size);

        compositor.push(Box::<EditorView>::default());
        compositor.push(Box::new(StatusLine {}));

        // Open files from arguments
        let mut args: Vec<String> = env::args().collect();
        while args.len() > 1 {
            let path = PathBuf::from(args.pop().unwrap());
            if let Ok(path) = path.canonicalize() {
                if path.is_file() {
                    match editor.open(PaneId::default(), &path, None) {
                        Ok(callback) => {
                            if let Some(cb) = callback {
                                let mut ctx = Context { editor: &mut editor };
                                cb(&mut compositor, &mut ctx);
                            }
                        },
                        Err(e) => editor.set_error(e.to_string()),
                    }
                } else if path.is_dir() {
                    // opening the files for multiple folders doesn't make sense
                    if compositor.find::<Files>().is_some() {
                        continue;
                    }
                    match Files::new(Some(&path)) {
                        Ok(f) => compositor.push(Box::new(f)),
                        Err(e) => editor.set_error(e.to_string()),
                    }
                }
            }
        }

        // Open a scratch buffer if no files are loaded
        if editor.documents.is_empty() {
            let id = editor.open_scratch(PaneId::default());
            editor.panes.load_doc_in_focus(id);
        }

        watch_file_changes(editor.tx.clone());

        Self { editor, compositor, terminal }
    }
}

impl Application {
    pub fn run(&mut self) -> Result<()> {
        terminal::enter_terminal_screen()?;
        self.event_loop()?;
        terminal::leave_terminal_screen()
    }

    fn event_loop(&mut self) -> Result<()> {
        self.draw()?;

        let tx = self.editor.tx.clone();

        thread::spawn(move || {
            while let Ok(event) = read() {
                _ = tx.send(Event::Term(event));
            }

            _ = tx.send(Event::Quit);
        });

        loop {
            match self.editor.rx.recv() {
                Ok(event) => match event {
                    Event::Draw => { self.draw()? },
                    Event::Quit => { break },
                    Event::BufferedInput(s) => {
                        if self.handle_buffered_input(s) {
                            self.draw()?
                        }
                    }
                    Event::Term(e) => {
                        if self.handle_crossterm_event(e) {
                            self.draw()?
                        }
                    },
                    Event::FileEvent(e) => {
                        if self.handle_file_event(e) {
                            self.draw()?
                        }
                    }
                },
                Err(err) => {
                    log::error!("Application channel hung up {err}");
                    break;
                },
            }
        }

        Ok(())
    }

    fn handle_crossterm_event(&mut self, event: crossterm::event::Event) -> bool {
        use crossterm::event::Event;

        match event {
            Event::Resize(width, height) => {
                let size = Rect::from((width, height));
                self.terminal.resize(size).expect("Couldn't resize the terminal");
                self.compositor.resize(size);
                true
            },
            Event::Key(KeyEvent { kind: KeyEventKind::Release, .. }) => false,
            Event::Key(_) | Event::Paste(_) => {
                let mut ctx = Context { editor: &mut self.editor };
                self.compositor.handle_event(event, &mut ctx)
            },
            Event::FocusGained => false,
            Event::FocusLost => false,
            Event::Mouse(_) => false,
        }
    }

    fn handle_buffered_input(&mut self, string: SmartString<LazyCompact>) -> bool {
        let mut ctx = Context { editor: &mut self.editor };
        self.compositor.handle_buffered_input(string.as_ref(), &mut ctx)
    }

    fn handle_file_event(&mut self, event: DebouncedEvent) -> bool {
        let mut ctx = Context { editor: &mut self.editor };
        self.compositor.handle_file_event(event, &mut ctx)
    }

    fn draw(&mut self) -> Result<()> {
        let mut ctx = Context { editor: &mut self.editor };

        self.compositor.render(self.terminal.current_buffer_mut(), &mut ctx);

        self.terminal.draw()?;

        if self.compositor.hide_cursor(&mut ctx) {
            self.terminal.hide_cursor()?;
        } else {
            self.terminal.show_cursor()?;
            if let (Some(position), style) = self.compositor.cursor(&mut ctx) {
                self.terminal.set_cursor(position, style.unwrap_or(SetCursorStyle::SteadyBlock))?;
            }
        }

        self.terminal.flush()
    }
}

fn watch_file_changes(app_tx: Sender<Event>) {
    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel();

        let mut debouncer = new_debouncer(Duration::from_secs(1), None, tx)
            .expect("can't create file watcher");
        let dir = std::env::current_dir().expect("Can't get current working dir");

        debouncer.watch(&dir, RecursiveMode::Recursive)
            .expect("Can't watch file changes in current working dir");

        for res in rx {
            match res {
                Ok(events) => {
                    for event in events {
                        if let Err(error) = app_tx.send(Event::FileEvent(event)) {
                            log::error!("Sending file event failed: {}", error);
                        }
                    }
                },
                Err(errors) => {
                    for error in errors {
                        log::error!("File watcher failed: {}", error);
                    }
                },
            }
        }
    });
}
