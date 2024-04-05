use crossterm::{
    cursor,
    event::{read, Event, KeyCode, KeyEvent},
    terminal, ExecutableCommand, QueueableCommand, style::{self, Color},
};
use std::{io::Write, env};
use anyhow::Result;

enum Mode {
    Normal,
    Insert,
}

enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    EnterNormalMode,
    EnterInsertModeAtCursor,
    AppendCharacter(char),
    InsertLineBelow,
    Quit,
}

fn handle_key_event(mode: &Mode, event: &KeyEvent) -> Option<Action> {
    match mode {
        Mode::Normal => handle_normal_mode_key_event(event),
        Mode::Insert => handle_insert_mode_key_event(event),
    }
}

fn handle_normal_mode_key_event(event: &KeyEvent) -> Option<Action> {
    match event.code {
        KeyCode::Char('h') => Some(Action::MoveLeft),
        KeyCode::Char('j') => Some(Action::MoveDown),
        KeyCode::Char('k') => Some(Action::MoveUp),
        KeyCode::Char('l') => Some(Action::MoveRight),
        KeyCode::Char('i') => Some(Action::EnterInsertModeAtCursor),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn handle_insert_mode_key_event(event: &KeyEvent) -> Option<Action> {
    match event.code {
        KeyCode::Esc => Some(Action::EnterNormalMode),
        KeyCode::Char(c) => Some(Action::AppendCharacter(c)),
        KeyCode::Enter => Some(Action::InsertLineBelow),
        _ => None,
    }
}

struct Renderer {
    stdout: std::io::Stdout,
    curr_buffer: RenderBuffer,
    prev_buffer: RenderBuffer,
    width: usize,
    height: usize,
}

impl Renderer {
    fn default() -> Result<Self> {
        let mut stdout = std::io::stdout();

        // terminal::enable_raw_mode()?;
        // stdout.execute(terminal::EnterAlternateScreen)?;
        // stdout.execute(terminal::Clear(terminal::ClearType::All))?;

        // let default_panic = std::panic::take_hook();
        // std::panic::set_hook(Box::new(move |info| {
        //     _ = std::io::stdout().execute(terminal::LeaveAlternateScreen);
        //     _ = terminal::disable_raw_mode();
        //     println!();
        //     default_panic(info);
        // }));

        let (width, height) = terminal::size()?;
        let width: usize = width.into();
        let height: usize = height.into();

        let curr_buffer = RenderBuffer::with_size(width, height);
        let prev_buffer = RenderBuffer::with_size(width, height);

        Ok(Self { stdout, curr_buffer, prev_buffer, width, height })
    }

    fn draw(&mut self, state: &Editor) -> Result<()> {
        // self.draw_buffer(state)?;
        self.draw_statusline();
        self.draw_cursor(state)?;
        self.paint()?;
        Ok(())
    }

    fn paint(&mut self) -> Result<()> {
        for Patch { cell: Cell { char, .. }, x, y } in self.curr_buffer.diff(&self.prev_buffer) {
            self.stdout.queue(cursor::MoveTo(x as u16, y as u16))?;
            self.stdout.queue(style::Print(char))?;
        }

        self.stdout.flush()?;
        // self.prev_buffer = std::mem::replace(&mut self.curr_buffer, RenderBuffer::with_size(self.width, self.height));

        Ok(())
    }

    fn draw_buffer(&mut self, state: &Editor) -> Result<()> {
        self.stdout.queue(cursor::MoveTo(0, 0))?;

        // TODO: clear the lines only when we need to scroll
        for y in 0..state.buffer_view.height {
            for x in 0..state.buffer_view.width {
                self.stdout.queue(cursor::MoveTo(x as u16, y as u16))?;
                self.stdout.queue(style::Print(" "))?;
            }
        }

        // draw the visible lines
        for y in 0..state.buffer_view.height {
            match state.buffer.lines.get(state.buffer_view.buffer_start_y + y) {
                None => break,
                Some(line) => {
                    for x in 0..state.buffer_view.width {
                        let buffer_x = x + state.buffer_view.buffer_start_x;
                        match line.chars().nth(buffer_x) {
                            None => break,
                            Some(c) => {
                                self.stdout.queue(cursor::MoveTo(x as u16, y as u16))?;
                                self.stdout.queue(style::Print(c))?;
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn draw_statusline(&mut self) {
        // self.stdout.queue(cursor::MoveTo(0, (self.height - 1).try_into().unwrap()))?;
        // self.stdout.queue(style::Print("Type 'q' to quit"))?;
        let label = "Type 'q' to quit";
        if label.len() < self.width {
            let chars: Vec<char> = label.chars().collect();
            self.curr_buffer.put_chars(&chars, 0, self.height - 1, None, None);
            for x in label.len()..self.width {
                self.curr_buffer.put_char(' ', x, self.height - 1, None, None);
            }
        }
    }

    fn draw_cursor(&mut self, state: &Editor) -> Result<()> {
        let x = state.buffer_view.cursor_x as u16;
        let y = state.buffer_view.cursor_y as u16;
        self.stdout.queue(cursor::MoveTo(x, y))?;

        match state.mode {
            Mode::Normal => self.stdout.queue(cursor::SetCursorStyle::SteadyBlock)?,
            Mode::Insert => self.stdout.queue(cursor::SetCursorStyle::SteadyBar)?,
        };

        Ok(())
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        _ = terminal::disable_raw_mode();
        _ = self.stdout.execute(terminal::LeaveAlternateScreen);
    }
}

struct ScrollableArea {
    width: usize,
    height: usize,
    cursor_x: usize,
    cursor_y: usize,
    buffer_start_y: usize,
    buffer_start_x: usize,
}

impl ScrollableArea {
    fn with_size(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            buffer_start_y: 0,
            buffer_start_x: 0,
        }
    }

    fn cursor_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y = self.cursor_y.saturating_sub(1);
        } else {
            self.scroll_up();
        }
    }

    fn scroll_up(&mut self) {
        if self.buffer_start_y > 0 {
            self.buffer_start_y = self.buffer_start_y.saturating_sub(1);
        }
    }

    fn cursor_down(&mut self, buffer: &Buffer) {
        if self.cursor_y < self.height.saturating_sub(1) {
            if buffer.lines.get(self.cursor_y + 1).is_some() {
                self.cursor_y += 1
            }
        } else {
            self.scroll_down(buffer)
        }
    }

    fn scroll_down(&mut self, buffer: &Buffer) {
        if self.buffer_start_y + self.height < buffer.lines.len() {
            self.buffer_start_y += 1;
        }
    }

    fn cursor_left(&mut self) {
        self.cursor_x = self.cursor_x.saturating_sub(1);
    }

    fn cursor_right(&mut self) {
        self.cursor_x += 1;
    }

    fn cursor_position_relative_to_buffer(&self) -> (usize, usize) {
        (
            self.buffer_start_x + self.cursor_x,
            self.buffer_start_y + self.cursor_y,
        )
    }
}

struct Editor {
    mode: Mode,
    buffer_view: ScrollableArea,
    buffer: Buffer,
}

impl Editor {
    fn with_size(width: usize, height: usize) -> Result<Self> {
        let mut args: Vec<String> = env::args().collect();

        let mut buffer = Buffer::default();
        if args.len() > 1 {
            let contents = std::fs::read_to_string(args.pop().unwrap())?;
            buffer = Buffer::from_contents(contents)
        }

        Ok(Self {
            buffer,
            mode: Mode::Normal,
            buffer_view: ScrollableArea::with_size(width, height),
        })
    }

    fn enter_normal_mode(&mut self) {
        self.mode = Mode::Normal;
        self.buffer_view.cursor_left();
    }

    fn enter_insert_mode(&mut self) {
        self.mode = Mode::Insert;
    }

    fn edit_buffer(&mut self, c: char) {
        let (cursor_x, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
        self.buffer.lines[cursor_y].insert(self.buffer_view.cursor_x, c);
        self.buffer_view.cursor_right();
    }

    fn insert_line_below(&mut self) {
        self.buffer.lines.insert(self.buffer_view.cursor_y + 1, "".to_string());
        self.buffer_view.cursor_down(&self.buffer);
        self.buffer_view.cursor_x = 0;
    }
}

struct Buffer {
    lines: Vec<String>,
}

impl Buffer {
    fn default() -> Self {
        Self { lines: vec!["".to_string()] }
    }

    fn from_contents(contents: String) -> Self {
        Self { lines: contents.lines().map(|l| l.to_string()).collect() }
    }
}

#[derive(PartialEq, Debug, Clone)]
struct Cell {
    char: char,
    fg: Option<Color>,
    bg: Option<Color>,
}

impl Cell {
    fn empty() -> Self {
        Self {
            char: ' ',
            fg: None,
            bg: None,
        }
    }
}

struct Patch {
    cell: Cell,
    x: usize,
    y: usize,
}

struct RenderBuffer {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
}

impl RenderBuffer {
    fn with_size(width: usize, height: usize) -> Self {
        let cells = vec![Cell::empty(); width * height];
        Self { width, height, cells }
    }

    fn diff(&self, other: &Self) -> Vec<Patch> {
        assert!(self.width == other.width && self.height == other.height);

        self.cells.iter()
            .zip(other.cells.iter())
            .enumerate()
            .filter(|(_, (a, b))| *a != *b)
            .map(|(i, (_, cell))| Patch {
                cell: cell.clone(),
                x: i % self.width,
                y: i / self.width,
            })
            .collect()
    }

    fn clear(&mut self) {
        self.cells.fill(Cell::empty());
    }

    fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.cells.resize(width * height, Cell::empty());
    }

    fn put_char(&mut self, char: char, x: usize, y: usize, fg: Option<Color>, bg: Option<Color>) {
        let index = self.width * y + x;

        if let Some(cell) = self.cells.get_mut(index) {
            *cell = Cell { char, fg, bg };
        }
    }

    fn put_chars(&mut self, chars: &[char], x: usize, y: usize, fg: Option<Color>, bg: Option<Color>) {
        let start = self.width * y + x;

        for (offset, &char) in chars.iter().enumerate() {
            if start + offset > self.cells.len() {
                break;
            }
            if let Some(cell) = self.cells.get_mut(start + offset) {
                *cell = Cell { char, fg, bg };
            }
        }
    }
}

fn main() -> Result<()> {
    let mut renderer = Renderer::default()?;

    let mut editor = Editor::with_size(
        renderer.width,
        renderer.height.saturating_sub(1),
    )?;

    let mut quit = false;

    while !quit {
        renderer.draw(&editor)?;
        match read()? {
            Event::Key(event) => {
                if let Some(action) = handle_key_event(&editor.mode, &event) {
                    match action {
                        Action::Quit                    => { quit = true; },
                        Action::MoveUp                  => { editor.buffer_view.cursor_up(); },
                        Action::MoveDown                => { editor.buffer_view.cursor_down(&editor.buffer); },
                        Action::MoveLeft                => { editor.buffer_view.cursor_left(); },
                        Action::MoveRight               => { editor.buffer_view.cursor_right(); },
                        Action::EnterInsertModeAtCursor => { editor.enter_insert_mode(); },
                        Action::EnterNormalMode         => { editor.enter_normal_mode(); },
                        Action::AppendCharacter(c)      => { editor.edit_buffer(c); },
                        Action::InsertLineBelow         => { editor.insert_line_below(); },
                    }
                }
            }
            // Event::Resize(height, width) => { state.resize(height, width) },
            Event::Resize(_, _) => todo!(),
            Event::Mouse(_)     => todo!(),
            Event::Paste(_)     => todo!(),
            Event::FocusGained  => todo!(),
            Event::FocusLost    => todo!(),
        }
    }

    // renderer.draw(&editor)?;

    // for (i, cell) in renderer.front_buffer.cells.iter().enumerate() {
    //     if cell.char == ' ' { continue; }
    //     let y = i / renderer.front_buffer.width;
    //     let x = i % renderer.front_buffer.width;
    //     println!("({x}, {y}) {:?}", cell);
    // }


    Ok(())
}
