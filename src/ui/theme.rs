macro_rules! style {
    ($color:literal) => {
        $crate::ui::style::Style::default().fg($crate::ui::theme::color($color).unwrap())
    };

    (
        { $($key:expr => $value:expr,)+ }
    ) => {
        {
            let mut style = $crate::ui::style::Style::default();
            $(
                style = match $key {
                    "fg" => style.fg($crate::ui::theme::color($value).unwrap()),
                    "bg" => style.bg($crate::ui::theme::color($value).unwrap()),
                    "ulc" => style.underline_color($crate::ui::theme::color($value).unwrap()),
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
            let mut scopes = vec![];
            $(
                let duplicate = styles.insert($key, style!($value));
                debug_assert!(duplicate.is_none(), "Duplicate theme key {}", stringify!($key));
                scopes.push($key);
            )+
            $crate::ui::theme::Theme { styles, scopes }
        }
    };
}

use std::collections::HashMap;
use once_cell::sync::Lazy;
use crate::language::syntax::Highlight;

use super::style::Style;
use crossterm::style::Color;
use anyhow::{anyhow, Result};

// Returns a crossterm Color from a str
pub fn color(str: &str) -> Result<Color> {
    match str {
        "reset"        => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Reset) },
        "black"        => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Black) },
        "dark_grey"    => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::DarkGrey) },
        "red"          => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Red) },
        "dark_red"     => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::DarkRed) },
        "green"        => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Green) },
        "dark_green"   => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::DarkGreen) },
        "yellow"       => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Yellow) },
        "dark_yellow"  => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::DarkYellow) },
        "blue"         => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Blue) },
        "dark_blue"    => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::DarkBlue) },
        "magenta"      => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Magenta) },
        "dark_magenta" => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::DarkMagenta) },
        "cyan"         => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Cyan) },
        "dark_cyan"    => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::DarkCyan) },
        "white"        => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::White) },
        "grey"         => match PALETTE.get(str) { Some(c) => color(c), None => Ok(Color::Grey) },
        s if s.starts_with('#') && s.len() >= 7 => {
            Ok(Color::Rgb {
                r: u8::from_str_radix(&s[1..3], 16).map_err(|_| anyhow!("Bad color hex value: {s}"))?,
                g: u8::from_str_radix(&s[3..5], 16).map_err(|_| anyhow!("Bad color hex value: {s}"))?,
                b: u8::from_str_radix(&s[5..7], 16).map_err(|_| anyhow!("Bad color hex value: {s}"))?,
            })
        },
        s if s.parse::<u8>().is_ok() => {
            Ok(Color::AnsiValue(s.parse::<u8>()?))
        },
        s => match PALETTE.get(s) {
            Some(c) => color(c),
            None => Err(anyhow!("Unknown color: {}", s))
        }
    }
}

pub struct Theme {
    styles: HashMap<&'static str, Style>,
    pub scopes: Vec<&'static str>,
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

    pub fn highlight_style(&self, highlight: Highlight) -> Style {
        self.get(self.scopes[highlight.0])
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
        ("selected", "#49473e"),

        ("magenta", "blossom"),
        ("green", "leaf"),
        ("red", "rose"),
        ("blue", "water"),
        ("cyan", "sky"),
    ])
});

pub static THEME: Lazy<Theme> = Lazy::new(|| {
    theme!(
        "ui" => "fg",
        "text" => "fg",
        "text.whitespace" => "muted1",
        "selection" => {
            "bg" => "selected",
        },

        "ui.border" => "muted",
        "ui.border.dialog" => "fg",
        "ui.text.dialog" => "fg",
        "ui.button" => {
            "fg" => "fg",
        },
        "ui.button.selected" => {
            "fg" => "fg",
            "mod" => "rev",
        },

        "ui.files.title" => {
            "fg" => "fg",
            "mod" => "bold",
        },
        "ui.files.folder" => {
            "fg" => "fg",
            "mod" => "bold",
        },
        "ui.files.marked" => {
            "bg" => "selected",
        },
        "ui.files.paste.copy" => "water",
        "ui.files.paste.move" => "muted",
        "ui.files.count" => "fg",
        "ui.files.search_match" => {
            "mod" => "italic",
            "ul" => "line",
            "fg" => "wood",
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
        "ui.statusline.normal" => {
            "fg" => "blue",
            "mod" => "rev",
        },
        "ui.statusline.insert" => {
            "fg" => "green",
            "mod" => "rev",
        },
        "ui.statusline.select" => {
            "fg" => "magenta",
            "mod" => "rev",
        },
        "ui.statusline.replace" => {
            "fg" => "yellow",
            "mod" => "rev",
        },
        "ui.statusline.modified" => "wood",
        "ui.statusline.read_only" => "muted",

        "ui.multicursor" => {
            "mod" => "rev",
        },
        "ui.multicursor.insert" => {
            "fg" => "green",
            "mod" => "rev",
        },
        "ui.multicursor.select" => {
            "fg" => "magenta",
            "mod" => "rev",
        },
        "ui.multicursor.replace" => {
            "ulc" => "yellow",
            "ul" => "line",
        },

        "ui.scrollbar" => {
            "fg" => "light_bg",
            "mod" => "rev",
        },

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
