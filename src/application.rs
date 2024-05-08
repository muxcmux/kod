use anyhow::Result;
use crossterm::{cursor::SetCursorStyle, event::{read, Event, KeyEvent, KeyEventKind}};
use crate::{compositor::{Compositor, Context}, editor::Editor, terminal::{self, Terminal}, ui::Rect, editor_view::{EditorView, StatusLine}};

pub struct Application {
    editor: Editor,
    compositor: Compositor,
    terminal: Terminal,
}

impl Application {
    pub fn new() -> Result<Self> {
        let size = crossterm::terminal::size().expect("Can't get terminal size");
        let size = Rect::from(size);

        let editor = Editor::new()?;
        let terminal = Terminal::new(size);
        let mut compositor = Compositor::new(size);

        compositor.push(Box::new(EditorView::new(size.clip_bottom(1))));
        compositor.push(Box::new(StatusLine {}));

        Ok(Self { editor, compositor, terminal })
    }

    pub fn run(&mut self) {
        _ = terminal::enter_terminal_screen();

        self.event_loop();

        _ = terminal::leave_terminal_screen();
    }

    fn event_loop(&mut self) {
        self.draw();

        loop {
            if self.editor.quit { break }

            match read() {
                Ok(event) => {
                    if self.handle_event(event) {
                        self.draw()
                    }
                }
                Err(_) => { break },
            }
        }
    }

    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Resize(width, height) => {
                let mut ctx = Context { editor: &mut self.editor };
                let size = Rect::from((width, height));
                self.terminal.resize(size);
                self.compositor.resize(size, &mut ctx);
                true
            },
            Event::Key(KeyEvent { kind: KeyEventKind::Release, .. }) => false,
            Event::Key(key_event) => {
                let mut ctx = Context { editor: &mut self.editor };
                self.compositor.handle_key_event(&key_event, &mut ctx);
                true
            },
            Event::FocusGained => false,
            Event::FocusLost => false,
            Event::Mouse(_) => false,
            Event::Paste(_) => false,
        }
    }

    fn draw(&mut self) {
        let mut ctx = Context { editor: &mut self.editor };

        self.compositor.render(self.terminal.current_buffer_mut(), &mut ctx);

        _ = self.terminal.draw();

        if let (Some(position), style) = self.compositor.cursor(&mut ctx) {
            _ = self.terminal.set_cursor(position, style.unwrap_or(SetCursorStyle::SteadyBlock));
        }

        _ = self.terminal.flush();
    }
}
