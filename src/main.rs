use anyhow::Result;
use crossterm::{
    cursor,
    event::{read, Event, KeyCode, KeyEvent, KeyEventKind},
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};
use std::{env, io::Write};

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
    EnterInsertModeAfterCursor,
    AppendCharacter(char),
    NewLine,
    InsertLineBelow,
    InsertLineAbove,
    DeleteCharacter,
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
        KeyCode::Char('a') => Some(Action::EnterInsertModeAfterCursor),
        KeyCode::Char('o') => Some(Action::InsertLineBelow),
        KeyCode::Char('O') => Some(Action::InsertLineAbove),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn handle_insert_mode_key_event(event: &KeyEvent) -> Option<Action> {
    match event.code {
        KeyCode::Esc => Some(Action::EnterNormalMode),
        KeyCode::Char(c) => {
            log::debug!("Char: {}", c);
            None
        },
        KeyCode::Enter => Some(Action::NewLine),
        KeyCode::Backspace => Some(Action::DeleteCharacter),
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

        terminal::enable_raw_mode()?;
        stdout.execute(terminal::EnterAlternateScreen)?;
        stdout.execute(terminal::Clear(terminal::ClearType::All))?;

        let default_panic = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            _ = std::io::stdout().execute(terminal::LeaveAlternateScreen);
            _ = terminal::disable_raw_mode();
            println!();
            default_panic(info);
        }));

        let (width, height) = terminal::size()?;
        let width: usize = width.into();
        let height: usize = height.into();

        let curr_buffer = RenderBuffer::with_size(width, height);
        let prev_buffer = RenderBuffer::with_size(width, height);

        Ok(Self {
            stdout,
            curr_buffer,
            prev_buffer,
            width,
            height,
        })
    }

    fn draw(&mut self, state: &Editor) -> Result<()> {
        self.draw_buffer(state);
        self.draw_statusline(state);
        self.paint()?;
        self.draw_cursor(state)?;
        self.stdout.flush()?;
        Ok(())
    }

    fn paint(&mut self) -> Result<()> {
        let patches = self.curr_buffer.diff(&self.prev_buffer);
        log::debug!("Patches: {}", patches.len());
        for Patch {
            cell: Cell { char, fg, bg },
            x,
            y,
        } in patches
        {
            self.stdout.queue(cursor::MoveTo(x as u16, y as u16))?;
            self.stdout.queue(style::PrintStyledContent(
                char.with(fg.unwrap_or(Color::Reset))
                    .on(bg.unwrap_or(Color::Reset)),
            ))?;
        }

        self.prev_buffer = std::mem::replace(
            &mut self.curr_buffer,
            RenderBuffer::with_size(self.width, self.height),
        );
        Ok(())
    }

    fn draw_buffer(&mut self, state: &Editor) {
        for y in 0..state.buffer_view.height {
            match state.buffer.lines.get(state.buffer_view.buffer_start_y + y) {
                None => break,
                Some(line) => {
                    for x in 0..state.buffer_view.width {
                        let buffer_x = x + state.buffer_view.buffer_start_x;
                        match line.chars().nth(buffer_x) {
                            None => break,
                            Some(c) => {
                                self.curr_buffer.put_char(c, x, y, None, None);
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw_statusline(&mut self, state: &Editor) {
        let label = match state.mode {
            Mode::Normal => " NOR ",
            Mode::Insert => " INS ",
        };

        let (cursor_x, cursor_y) = state.buffer_view.cursor_position_relative_to_buffer();
        let position = format!(" {}:{} ", cursor_y + 1, cursor_x + 1);

        if label.len() + position.len() < self.width {
            let chars: Vec<char> = label.chars().collect();
            self.curr_buffer.put_chars(
                &chars,
                0,
                self.height - 1,
                Some(Color::Black),
                Some(Color::White),
            );
            for x in label.len()..self.width - position.len() {
                self.curr_buffer
                    .put_char(' ', x, self.height - 1, None, None);
            }
            let position_chars: Vec<char> = position.chars().collect();
            self.curr_buffer.put_chars(
                &position_chars,
                self.width - position.len(),
                self.height - 1,
                Some(Color::Black),
                Some(Color::White),
            );
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

struct ScrollView {
    width: usize,
    height: usize,
    cursor_x: usize,
    cursor_y: usize,
    sticky_cursor_x: usize,
    buffer_start_y: usize,
    buffer_start_x: usize,
}

impl ScrollView {
    fn with_size(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            sticky_cursor_x: 0,
            buffer_start_y: 0,
            buffer_start_x: 0,
        }
    }

    fn adjust_cursor_x_after_vertical_move(&mut self, buffer: &Buffer) {
        let (_cursor_x, cursor_y) = self.cursor_position_relative_to_buffer();
        let current_line = buffer.lines.get(cursor_y).unwrap();
        self.cursor_x = self.sticky_cursor_x.min(current_line.len());
    }

    fn cursor_up(&mut self, buffer: &Buffer) {
        if self.cursor_y > 0 {
            self.cursor_y = self.cursor_y.saturating_sub(1);
        } else {
            self.scroll_up();
        }
        self.adjust_cursor_x_after_vertical_move(buffer);
    }

    fn cursor_down(&mut self, buffer: &Buffer) {
        if self.cursor_y < self.height.saturating_sub(1) {
            if buffer.lines.get(self.cursor_y + 1).is_some() {
                self.cursor_y += 1
            }
        } else {
            self.scroll_down(buffer)
        }
        self.adjust_cursor_x_after_vertical_move(buffer);
    }

    fn scroll_up(&mut self) {
        if self.buffer_start_y > 0 {
            self.buffer_start_y = self.buffer_start_y.saturating_sub(1);
        }
    }

    fn scroll_down(&mut self, buffer: &Buffer) {
        if self.buffer_start_y + self.height < buffer.lines.len() {
            self.buffer_start_y += 1;
        }
    }

    fn cursor_left(&mut self) {
        if self.cursor_x > 0 {
            self.cursor_x = self.cursor_x.saturating_sub(1);
        } else {
            //self.scroll_left();
        }
        self.sticky_cursor_x = self.cursor_x;
    }

    fn cursor_right(&mut self, buffer: &Buffer) {
        let (cursor_x, cursor_y) = self.cursor_position_relative_to_buffer();
        let current_line = buffer.lines.get(cursor_y).unwrap();

        if self.cursor_x < self.width {
            if cursor_x < current_line.len() {
                self.cursor_x += 1;
            }
        } else {
            //self.scroll_right(buffer);
        }
        self.sticky_cursor_x = self.cursor_x;
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
    buffer_view: ScrollView,
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
            buffer_view: ScrollView::with_size(width, height),
        })
    }

    fn enter_normal_mode(&mut self) {
        if let Mode::Insert = self.mode {
            self.buffer_view.cursor_left();
        }
        self.mode = Mode::Normal;
    }

    fn enter_insert_mode_relative_to_cursor(&mut self, x: usize) {
        for _ in 0..x {
            self.buffer_view.cursor_right(&self.buffer);
        }
        self.mode = Mode::Insert;
    }

    fn append_character(&mut self, c: char) {
        let (cursor_x, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
        self.buffer.lines[cursor_y].insert(cursor_x, c);
        self.buffer_view.cursor_right(&self.buffer);
    }

    fn delete_character(&mut self) {
        let (cursor_x, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
        if cursor_x > 0 {
            self.buffer.lines[cursor_y].remove(cursor_x - 1);
            self.buffer_view.cursor_left();
        }
    }

    fn insert_line_break(&mut self) {
        let (cursor_x, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
        let right = self.buffer.lines[cursor_y].split_off(cursor_x);
        self.insert_line_relative_to_cursor(1, right);
    }

    fn insert_line_below(&mut self) {
        self.insert_line_relative_to_cursor(1, "".to_string());
    }

    fn insert_line_above(&mut self) {
        self.insert_line_relative_to_cursor(0, "".to_string());
    }

    fn insert_line_relative_to_cursor(&mut self, y: usize, contents: String) {
        let (_, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
        self.buffer.lines.insert(cursor_y + y, contents);
        for _ in 0..y {
            self.buffer_view.cursor_down(&self.buffer);
        }
        self.buffer_view.cursor_x = 0;
        self.mode = Mode::Insert;
    }
}

struct Buffer {
    lines: Vec<String>,
}

impl Buffer {
    fn default() -> Self {
        Self {
            lines: vec!["".to_string()],
        }
    }

    fn from_contents(contents: String) -> Self {
        Self {
            lines: contents.lines().map(|l| l.to_string()).collect(),
        }
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

#[derive(Clone, Debug)]
struct RenderBuffer {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
}

impl RenderBuffer {
    fn with_size(width: usize, height: usize) -> Self {
        let cells = vec![Cell::empty(); width * height];
        Self {
            width,
            height,
            cells,
        }
    }

    fn diff(&self, other: &Self) -> Vec<Patch> {
        assert!(self.width == other.width && self.height == other.height);

        self.cells
            .iter()
            .zip(other.cells.iter())
            .enumerate()
            .filter(|(_, (a, b))| *a != *b)
            .map(|(i, (cell, _))| Patch {
                cell: cell.clone(),
                x: i % self.width,
                y: i / self.width,
            })
            .collect()
    }

    // fn resize(&mut self, width: usize, height: usize) {
    //     self.width = width;
    //     self.height = height;
    //     self.cells.resize(width * height, Cell::empty());
    // }

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

fn setup_logging() -> Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}: {}",
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(fern::log_file("log.log")?)
        .apply()?;

    Ok(())
}

fn main() -> Result<()> {
    setup_logging()?;

    let mut renderer = Renderer::default()?;

    let mut editor = Editor::with_size(renderer.width, renderer.height.saturating_sub(1))?;

    loop {
        renderer.draw(&editor)?;

        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Press {
                    if let Some(action) = handle_key_event(&editor.mode, &event) {
                        match action {
                            Action::Quit => break,
                            Action::MoveUp => {
                                editor.buffer_view.cursor_up(&editor.buffer);
                            }
                            Action::MoveDown => {
                                editor.buffer_view.cursor_down(&editor.buffer);
                            }
                            Action::MoveLeft => {
                                editor.buffer_view.cursor_left();
                            }
                            Action::MoveRight => {
                                editor.buffer_view.cursor_right(&editor.buffer);
                            }
                            Action::EnterInsertModeAtCursor => {
                                editor.enter_insert_mode_relative_to_cursor(0);
                            }
                            Action::EnterInsertModeAfterCursor => {
                                editor.enter_insert_mode_relative_to_cursor(1);
                            }
                            Action::EnterNormalMode => {
                                editor.enter_normal_mode();
                            }
                            Action::NewLine => {
                                editor.insert_line_break();
                            }
                            Action::InsertLineBelow => {
                                editor.insert_line_below();
                            }
                            Action::InsertLineAbove => {
                                editor.insert_line_above();
                            }
                            Action::AppendCharacter(c) => {
                                editor.append_character(c);
                            }
                            Action::DeleteCharacter => {
                                editor.delete_character();
                            }
                        }
                    }
                }
            }
            Event::Resize(_, _) => todo!(),
            Event::Mouse(_) => todo!(),
            Event::Paste(_) => todo!(),
            Event::FocusGained => todo!(),
            Event::FocusLost => todo!(),
        }
    }

    Ok(())
}
