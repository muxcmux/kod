use std::io::{stdout, Write};

use crate::ui::{Buffer, Patch, Position, Rect};
use anyhow::Result;
use crossterm::{cursor::{self, SetCursorStyle}, style::{Color, Print, SetBackgroundColor, SetForegroundColor}, terminal::{self, Clear, ClearType}, ExecutableCommand, QueueableCommand};

pub fn enter_terminal_screen() -> Result<()> {
    let mut stdout = std::io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        _ = leave_terminal_screen();
        println!();
        default_panic(info);
    }));

    Ok(())
}

pub fn leave_terminal_screen() -> Result<()> {
    terminal::disable_raw_mode()?;
    stdout().execute(terminal::LeaveAlternateScreen)?;

    Ok(())
}

pub struct Terminal {
    buffers: [Buffer; 2],
    current: usize,
    size: Rect,
}

impl Terminal {
    pub fn new(size: Rect) -> Self {
        let buffers = [
            Buffer::new(size),
            Buffer::new(size)
        ];

        Self {
            buffers,
            current: 0,
            size,
        }
    }

    pub fn current_buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.current]
    }

    pub fn resize(&mut self, size: Rect) -> Result<()> {
        self.buffers[self.current].resize(size);
        self.buffers[1 - self.current].resize(size);
        self.size = size;
        self.clear()
    }

    pub fn clear(&mut self) -> Result<()> {
        stdout().execute(Clear(ClearType::All))?;
        self.buffers[1 - self.current].reset();

        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        stdout().flush()?;

        Ok(())
    }

    pub fn draw(&mut self) -> Result<()> {
        let mut stdout = stdout();

        let prev_buffer = &self.buffers[1 - self.current];
        let curr_buffer = &self.buffers[self.current];

        let mut fg = Color::Reset;
        let mut bg = Color::Reset;

        for Patch { cell, x, y, } in prev_buffer.diff(curr_buffer) {
            stdout.queue(cursor::MoveTo(x as u16, y as u16))?;

            if cell.fg != fg {
                stdout.queue(SetForegroundColor(cell.fg))?;
                fg = cell.fg;
            }
            if cell.bg != bg {
                stdout.queue(SetBackgroundColor(cell.bg))?;
                bg = cell.bg;
            }

            stdout.queue(Print(&cell.symbol))?;
        }

        stdout.queue(SetForegroundColor(Color::Reset))?;
        stdout.queue(SetBackgroundColor(Color::Reset))?;

        self.buffers[1 - self.current].reset();
        self.current = 1 - self.current;

        Ok(())
    }

    pub fn set_cursor(&self, position: Position, style: SetCursorStyle) -> Result<()> {
        let mut stdout = stdout();
        stdout.queue(cursor::MoveTo(position.x, position.y))?;
        stdout.queue(style)?;
        Ok(())
    }
}
