use crossterm::{cursor::SetCursorStyle, event::{read, Event, KeyEvent, KeyEventKind}};
use crate::{command_line::CommandLine, compositor::{Compositor, Context}, editor::Editor, editor_view::EditorView, status_line::StatusLine, terminal::{self, Terminal}, ui::Rect};
use anyhow::Result;

pub struct Application {
    editor: Editor,
    compositor: Compositor,
    terminal: Terminal,
}

impl Default for Application {
    fn default() -> Self {
        let size = crossterm::terminal::size().expect("Can't get terminal size");
        let size = Rect::from(size);

        let mut editor = Editor::default();
        let terminal = Terminal::new(size);
        let mut compositor = Compositor::new(size);

        let ctx = Context { editor: &mut editor };

        compositor.push(Box::new(EditorView::new(size.clip_bottom(2), &ctx)));
        compositor.push(Box::new(StatusLine::new(size.clip_top(size.height.saturating_sub(2)))));
        compositor.push(Box::new(CommandLine::new(size.clip_top(size.height.saturating_sub(1)))));

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

        loop {
            if self.editor.quit { break }

            match read() {
                Ok(event) => {
                    if self.handle_event(event) {
                        self.draw()?
                    }
                }
                Err(_) => { break },
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Resize(width, height) => {
                let mut ctx = Context { editor: &mut self.editor };
                let size = Rect::from((width, height));
                self.terminal.resize(size).expect("Couldn't resize the terminal");
                self.compositor.resize(size, &mut ctx);
                true
            },
            Event::Key(KeyEvent { kind: KeyEventKind::Release, .. }) => false,
            Event::Key(key_event) => {
                let mut ctx = Context { editor: &mut self.editor };
                self.compositor.handle_key_event(key_event, &mut ctx)
            },
            Event::FocusGained => false,
            Event::FocusLost => false,
            Event::Mouse(_) => false,
            Event::Paste(_) => false,
        }
    }

    fn draw(&mut self) -> Result<()> {
        let mut ctx = Context { editor: &mut self.editor };

        self.compositor.render(self.terminal.current_buffer_mut(), &mut ctx);

        self.terminal.draw()?;

        if let (Some(position), style) = self.compositor.cursor(&mut ctx) {
            self.terminal.set_cursor(position, style.unwrap_or(SetCursorStyle::SteadyBlock))?;
        }

        self.terminal.flush()
    }
}
