use crate::ui::Position;
use crate::ui::buffer::Buffer;
use crate::ui::Rect;
use std::any::Any;

use crossterm::{cursor::SetCursorStyle, event::{Event, KeyEvent}};

use crate::editor::Editor;

pub struct Context<'a> {
    pub editor: &'a mut Editor
}

pub type Callback = Box<dyn FnOnce(&mut Compositor, &mut Context)>;

pub enum EventResult {
    Ignored(Option<Callback>),
    Consumed(Option<Callback>),
}

pub trait Component: Any + AnyComponent {
    fn handle_key_event(&mut self, _event: KeyEvent, _ctx: &mut Context) -> EventResult {
        EventResult::Ignored(None)
    }

    fn handle_paste(&mut self, _str: &str, _ctx: &mut Context) -> EventResult {
        EventResult::Ignored(None)
    }

    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context);

    fn cursor(&self, _area: Rect, _ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (None, None)
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        false
    }

    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
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

    pub fn pop(&mut self) -> Option<Box<dyn Component>>  {
        self.layers.pop()
    }

    pub fn render(&mut self, buffer: &mut Buffer, ctx: &mut Context) {
        for layer in &mut self.layers {
            layer.render(self.size, buffer, ctx);
        }
    }

    pub fn resize(&mut self, size: Rect) {
        self.size = size;
    }

    pub fn hide_cursor(&self, ctx: &mut Context) -> bool {
        for layer in self.layers.iter().rev() {
            if layer.hide_cursor(ctx) {
                return true;
            }
        }
        false
    }

    pub fn cursor(&self, ctx: &mut Context) -> (Option<Position>, Option<SetCursorStyle>) {
        for layer in self.layers.iter().rev() {
            if let (Some(pos), kind) = layer.cursor(self.size, ctx) {
                return (Some(pos), kind);
            }
        }
        (None, None)
    }

    pub fn handle_event(&mut self, event: Event, ctx: &mut Context) -> bool {
        let mut callbacks = vec![];
        let mut consumed = false;

        for layer in self.layers.iter_mut().rev() {
            let result = match event {
                Event::Key(key_event) => layer.handle_key_event(key_event, ctx),
                Event::Paste(ref s) => layer.handle_paste(s, ctx),
                _ => unreachable!()
            };
            match result {
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

    pub fn find<T: 'static>(&mut self) -> Option<&mut T> {
        let type_name = std::any::type_name::<T>();
        self.layers
            .iter_mut()
            .find(|component| component.type_name() == type_name)
            .and_then(|component| component.as_any_mut().downcast_mut())
    }

    pub fn remove<T: 'static>(&mut self) -> Option<Box<dyn Component>> {
        let type_name = std::any::type_name::<T>();
        let idx = self
            .layers
            .iter()
            .position(|component| component.type_name() == type_name)?;
        Some(self.layers.remove(idx))
    }
}

/// This trait is automatically implemented for any `T: Component`.
pub trait AnyComponent {
    /// Downcast self to a `Any`.
    fn as_any(&self) -> &dyn Any;

    /// Downcast self to a mutable `Any`.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Returns a boxed any from a boxed self.
    ///
    /// Can be used before `Box::downcast()`.
    fn as_boxed_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<T: Component> AnyComponent for T {
    /// Downcast self to a `Any`.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Downcast self to a mutable `Any`.
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn as_boxed_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}
