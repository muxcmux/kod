use std::thread;

use crossterm::{cursor::SetCursorStyle, event::{read, KeyEvent, KeyEventKind}};
use crate::{components::{editor_view::EditorView, status_line::StatusLine}, compositor::{Compositor, Context}, editor::Editor, ui::{terminal::{self, Terminal}, Rect}};
use anyhow::Result;

pub enum Event {
    Draw,
    Quit,
    Term(crossterm::event::Event),
}

pub struct Application {
    editor: Editor,
    compositor: Compositor,
    terminal: Terminal,
}

impl Default for Application {
    fn default() -> Self {
        let size = crossterm::terminal::size().expect("Can't get terminal size");
        let size = Rect::from(size);

        let editor = Editor::new(size);
        let terminal = Terminal::new(size);
        let mut compositor = Compositor::new(size);

        compositor.push(Box::<EditorView>::default());
        compositor.push(Box::new(StatusLine {}));

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
                    Event::Term(e) => {
                        if self.handle_crossterm_event(e) {
                            self.draw()?
                        }
                    },
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
