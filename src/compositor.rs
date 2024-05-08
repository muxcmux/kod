use crossterm::{cursor::SetCursorStyle, event::KeyEvent};

use crate::{editor::Editor, ui::{Buffer, Position, Rect}};

pub struct Context<'a> {
    pub editor: &'a mut Editor
}

pub type Callback = Box<dyn FnOnce(&mut Compositor, &mut Context)>;

pub enum EventResult {
    Ignored(Option<Callback>),
    Consumed(Option<Callback>),
}

pub trait Component {
    fn handle_key_event(&mut self, _event: &KeyEvent, _ctx: &mut Context) -> EventResult {
        EventResult::Ignored(None)
    }

    fn resize(&mut self, new_size: Rect, ctx: &mut Context);

    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context);

    fn cursor(&self, _area: Rect, _ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (None, None)
    }
}

pub struct Compositor {
    size: Rect,
    layers: Vec<Box<dyn Component>>,
}

impl Compositor {
    pub fn new(size: Rect) -> Self {
        Self { size, layers: vec![] }
    }

    pub fn push(&mut self, layer: Box<dyn Component>) {
        self.layers.push(layer);
    }

    pub fn render(&mut self, buffer: &mut Buffer, ctx: &mut Context) {
        for layer in &mut self.layers {
            layer.render(self.size, buffer, ctx);
        }
    }

    pub fn resize(&mut self, size: Rect, ctx: &mut Context) {
        self.size = size;
        for layer in &mut self.layers {
            layer.resize(size, ctx);
        }
    }

    pub fn cursor(&self, ctx: &mut Context) -> (Option<Position>, Option<SetCursorStyle>) {
        for layer in self.layers.iter().rev() {
            if let (Some(pos), kind) = layer.cursor(self.size, ctx) {
                return (Some(pos), kind);
            }
        }
        (None, None)
    }

    pub fn handle_key_event(&mut self, event: &KeyEvent, ctx: &mut Context) -> bool {
        let mut callbacks = vec![];
        let mut consumed = false;

        for layer in self.layers.iter_mut().rev() {
            match layer.handle_key_event(event, ctx) {
                EventResult::Consumed(callback) => {
                    if let Some(cb) = callback { callbacks.push(cb) }
                    consumed = true;
                    break;
                }
                EventResult::Ignored(callback) => {
                    if let Some(cb) = callback { callbacks.push(cb) }
                }
            };
        }

        for callback in callbacks {
            callback(self, ctx)
        }

        consumed
    }
}
