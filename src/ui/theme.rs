macro_rules! style {
    ($color:literal) => {
        $crate::ui::style::Style::default().fg($crate::ui::theme::color($color))
    };

    (
        { $($key:expr => $value:expr,)+ }
    ) => {
        {
            let mut style = $crate::ui::style::Style::default();
            $(
                style = match $key {
                    "fg" => style.fg($crate::ui::theme::color($value)),
                    "bg" => style.bg($crate::ui::theme::color($value)),
                    "ulc" => style.underline_color($crate::ui::theme::color($value)),
                    "ul" => style.underline_style($value.parse().unwrap_or_else(|_| panic!("Invalid ul style: {}", $value))),
                    "mod" => {
                        if $value.starts_with('-') {
                            let modifier = $value[1..$value.len()].parse().unwrap_or_else(|_| panic!("Invalid mod: {}", $value));
                            style.remove_modifier(modifier)
                        } else {
                            let modifier = $value.parse().unwrap_or_else(|_| panic!("Invalid mod: {}", $value));
                            style.add_modifier(modifier)
                        }
                    },
                    _ => style,
                };
            )+
            style
        }
    };
}

macro_rules! theme {
    (
        $($key:literal => $value:tt,)+
    ) => {
        {
            let mut styles = std::collections::HashMap::new();
            $(
                let duplicate = styles.insert($key, style!($value));
                debug_assert!(duplicate.is_none(), "Duplicate theme key {}", stringify!($key));
            )+
            $crate::ui::theme::Theme { styles }
        }
    };
}

use std::collections::HashMap;
use once_cell::sync::Lazy;
use crate::language::syntax::Highlight;

use super::style::Style;
use crossterm::style::Color;

// Returns a crossterm Color from a str
fn color(str: &str) -> Color {
    match str {
        "reset"        => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Reset),
        "black"        => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Black),
        "dark_grey"    => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::DarkGrey),
        "red"          => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Red),
        "dark_red"     => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::DarkRed),
        "green"        => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Green),
        "dark_green"   => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::DarkGreen),
        "yellow"       => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Yellow),
        "dark_yellow"  => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::DarkYellow),
        "blue"         => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Blue),
        "dark_blue"    => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::DarkBlue),
        "magenta"      => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Magenta),
        "dark_magenta" => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::DarkMagenta),
        "cyan"         => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Cyan),
        "dark_cyan"    => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::DarkCyan),
        "white"        => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::White),
        "grey"         => PALETTE.get(str).map(|c| color(c)).unwrap_or(Color::Grey),
        s if s.starts_with('#') && s.len() >= 7 => {
            Color::Rgb {
                r: u8::from_str_radix(&s[1..3], 16).unwrap_or_else(|_| panic!("Bad color hex value: {s}")),
                g: u8::from_str_radix(&s[3..5], 16).unwrap_or_else(|_| panic!("Bad color hex value: {s}")),
                b: u8::from_str_radix(&s[5..7], 16).unwrap_or_else(|_| panic!("Bad color hex value: {s}")),
            }
        },
        s if s.parse::<u8>().is_ok() => {
            Color::AnsiValue(s.parse::<u8>().unwrap())
        },
        s => PALETTE.get(s).map(|c| color(c)).unwrap_or_else(|| panic!("Unknown color: {}", s)),
    }
}

pub struct Theme {
    styles: HashMap<&'static str, Style>
}

impl Theme {
    pub fn get(&self, scope: &str) -> Style {
        self.try_get(scope).unwrap_or_default()
    }

    /// Get the style of a scope, falling back to dot separated broader
    /// scopes. For example if `ui.text.focus` is not defined in the theme,
    /// `ui.text` is tried and then `ui` is tried.
    pub fn try_get(&self, scope: &str) -> Option<Style> {
        std::iter::successors(Some(scope), |s| Some(s.rsplit_once('.')?.0))
            .find_map(|s| self.styles.get(s).copied())
    }

    pub fn scopes(&self) -> &[&str] {
        &[
            "comment",
            "operator",
            "punctuation",
            "variable",
            "variable.builtin",
            "constant.numeric",
            "constant",
            "attributes",
            "type",
            "string",
            "variable.other.member",
            "constant.character.escape",
            "function",
            "constructor",
            "special",
            "keyword",
            "label",
            "namespace",

            "markup.heading",
            "markup.list",
            "markup.bold",
            "markup.italic",
            "markup.link.url",
            "markup.link.text",
            "markup.quote",
            "markup.raw",

            "diff.plus",
            "diff.delta",
            "diff.minus",
        ]
    }

    pub fn highlight_style(&self, highlight: Highlight) -> Style {
        self.get(self.scopes()[highlight.0])
    }
}

// kanagawabones
pub static PALETTE: Lazy<HashMap<&str, &str>> = Lazy::new(|| {
    HashMap::from([
        ("fg", "#ddd8bb"),
        ("bg", "#1f1f28"),
        ("light_bg", "#363644"),
        ("muted", "#646476"),
        ("muted1", "#696977"),
        ("rose", "#e46876"),
        ("leaf", "#98bc6d"),
        ("wood", "#e5c283"),
        ("water", "#7fb4ca"),
        ("blossom","#957fb8"),
        ("sky", "#7eb3c9"),
    ])
});

pub static THEME: Lazy<Theme> = Lazy::new(|| {
    theme!(
        "text" => "fg",
        "text.whitespace" => "muted1",
        "selection" => {
            "bg" => "#49473e",
        },

        "ui.pane.border" => "muted",
        "ui.dialog.border" => "fg",
        "ui.dialog.text" => "fg",
        "ui.dialog.button" => {
            "fg" => "fg",
            "mod" => "bold",
        },
        "ui.dialog.button.selected" => {
            "mod" => "rev",
            "mod" => "bold",
        },

        "ui.menu" => "muted1",
        "ui.menu.selected" => "fg",

        "ui.linenr" => "muted",
        "ui.linenr.selected" => {
            "fg" => "fg",
            "mod" => "bold",
        },

        "ui.text_input" => "fg",
        "ui.text_input.blur" => "muted1",

        "ui.statusline" => {
            "bg" => "light_bg",
        },
        "ui.statusline.modified" => "wood",
        "ui.statusline.read_only" => "muted",

        "comment" => "muted",
        "operator" => "wood",
        "punctuation" => "#7d7d8d",
        "variable" => "#bbb79e",
        "variable.builtin" => {
            "fg" => "#a29e89",
            "mod" => "italic",
        },
        "constant.numeric" => "wood",
        "constant" => "#bbb79e",
        "attributes" => "wood",
        "type" => "#9797a5",
        "string"  => "leaf",
        "variable.other.member" => "#bbb79e",
        "constant.character.escape" => "cyan",
        "function" => "fg",
        "constructor" => "#bbb79e",
        "special" => "water",
        "keyword" => "wood",
        "label" => "sky",
        "namespace" => {
            "fg" => "#a29e89",
            "mod" => "italic",
        },

        "markup.heading" => "blue",
        "markup.list" => "red",
        "markup.bold" => {
            "fg" => "wood",
            "mod" => "bold",
        },
        "markup.italic" => {
            "fg" => "magenta",
            "mod" => "italic",
        },
        "markup.link.url" => {
            "fg" => "yellow",
            "ul" => "line",
        },
        "markup.link.text" => "red",
        "markup.quote" => "cyan",
        "markup.raw" => "green",

        "diff.plus" => "green",
        "diff.delta" => "yellow",
        "diff.minus" => "red",

        "diagnostic" => {
            "ul" => "curl",
        },

        "info" => "water",
        "hint" => "sky",
        "warning" => "wood",
        "error" => "rose",
    )
});
