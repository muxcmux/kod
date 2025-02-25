use crate::keymap::map;
use super::Keymap;

pub fn normal_mode_keymap() -> Keymap {
    map!({
        "esc" => clean_state,
        ":" => command_palette,
        "R" => enter_replace_mode,
        "v" => enter_select_mode,

        "minus" => open_files,

        "h" => move_left,
        "j" => move_down,
        "k" => move_up,
        "l" => move_right,
        "C-u" | "pageup" => half_page_up,
        "C-d" | "pagedown" => half_page_down,
        "w" => goto_word_start_forward,
        "W" => goto_long_word_start_forward,
        "b" => goto_word_start_backward,
        "B" => goto_long_word_start_backward,
        "e" => goto_word_end_forward,
        "E" => goto_long_word_end_forward,
        "t" => goto_until_character_forward,
        "f" => goto_character_forward,
        "T" => goto_until_character_backward,
        "F" => goto_character_backward,
        ";" => repeat_goto_character_next,
        "," => repeat_goto_character_prev,

        "A-j" => add_cursor_below,
        "A-k" => add_cursor_above,
        "A-l" => add_cursor_next_word,
        "A-h" => add_cursor_prev_word,

        "C-w" => {
            "h" | "C-h" => switch_pane_left,
            "l" | "C-l" => switch_pane_right,
            "k" | "C-k" => switch_pane_top,
            "j" | "C-j" => switch_pane_bottom,
            "w" | "C-w" => switch_to_last_pane,
        },

        "^" | "home" | "C-h" => goto_line_first_non_whitespace,
        "$" | "end" | "C-l" => goto_eol,
        "G" => goto_last_line,

        "g" => {
            "g" => goto_first_line,
            "e" => goto_word_end_backward,
            "E" => goto_long_word_end_backward,
            // ";" => goto_prev_edit,
            // "," => goto_next_edit,
        },

        "u" => undo,
        "C-r" => redo,

        "/" => search,
        "n" => next_search_match,
        "N" => prev_search_match,

        "i" => enter_insert_mode_at_cursor,
        "I" => enter_insert_mode_at_first_non_whitespace,
        "a" => enter_insert_mode_after_cursor,
        "A" => enter_insert_mode_at_eol,
        "o" => insert_line_below,
        "O" => insert_line_above,

        "D" => delete_until_eol,
        "X" => delete_symbol_to_the_left,
        "d" =>  {
            "d" => delete_current_line,
            "h" => delete_symbol_to_the_left,
            // "l" => delete_symbol_to_the_right,
            // "j" => delete_line_below,
            // "k" => delete_line_above,
            // "w" | "e" => delete_word,
            // "b" => delete_word_backwards,
            // "W" => delete_long_word,
            // "B" => delete_long_word_backwards,
            // "t" => delete_until_character_forward,
            // "f" => delete_character_forward,
            // "T" => delete_until_character_backward,
            // "F" => delete_character_backward,
            "C-l" | "$" | "end" => delete_until_eol,
            // "C-h" => delete_until_first_non_whitespace,
            // "G" => delete_until_last_line,
            // "g" => {
            //      "g" => delete_until_first_line,
            // }
            "i" => delete_text_object_inside,
            // "a" => delete_text_object_around,
        },

        "C" => change_until_eol,
        "c" =>  {
            "c" => change_current_line,
            "h" => change_symbol_to_the_left,
            // "l" => change_symbol_to_the_right,
            // "j" => change_line_below,
            // "k" => change_line_above,
            // "w" | "e" => change_word,
            // "b" => change_word_backwards,
            // "W" => change_long_word,
            // "B" => change_long_word_backwards,
            // "t" => change_until_character_forward,
            // "f" => change_character_forward,
            // "T" => change_until_character_backward,
            // "F" => change_character_backward,
            "C-l" | "$" | "end" => change_until_eol,
            // "C-h" => change_until_first_non_whitespace,
            // "G" => change_until_last_line,
            // "g" => {
            //      "g" => change_until_first_line,
            // }
            "i" => change_text_object_inside,
            // "a" => change_text_object_around,
        },
    })
}

pub fn insert_mode_keymap() -> Keymap {
    map!({
        "esc" => enter_normal_mode,

        "left" => move_left,
        "down" => move_down,
        "up" => move_up,
        "right" => move_right,

        "S-right" => goto_word_start_forward,
        "S-left" => goto_word_start_backward,

        "C-h" | "home" => goto_line_first_non_whitespace,
        "C-l" | "end" => goto_eol,

        "j" => {
            "k" => enter_normal_mode,
        },

        "backspace" => delete_symbol_to_the_left,
    })
}

pub fn replace_mode_keymap() -> Keymap {
    map!({
        "esc" => enter_normal_mode,

        "left" | "backspace" => move_left,
        "down" => move_down,
        "up" => move_up,
        "right" => move_right,

        "C-h" | "home" => goto_line_first_non_whitespace,
        "C-l" | "end" => goto_eol,

        "j" => {
            "k" => enter_normal_mode,
        },
    })
}

pub fn select_mode_keymap() -> Keymap {
    map!({
        "esc" => enter_normal_mode,
        "v" => expand_selection_to_whole_lines,

        "h" => move_left,
        "j" => move_down,
        "k" => move_up,
        "l" => move_right,
        "C-u" | "pageup" => half_page_up,
        "C-d" | "pagedown" => half_page_down,
        "w" | "S-right" => goto_word_start_forward,
        "W" => goto_long_word_start_forward,
        "b" | "S-left" => goto_word_start_backward,
        "B" => goto_long_word_start_backward,
        "e" => goto_word_end_forward,
        "E" => goto_long_word_end_forward,
        "t" => goto_until_character_forward,
        "f" => goto_character_forward,
        "T" => goto_until_character_backward,
        "F" => goto_character_backward,
        ";" => repeat_goto_character_next,
        "," => repeat_goto_character_prev,

        "^" | "home" | "C-h" => goto_line_first_non_whitespace,
        "$" | "end" | "C-l" => goto_eol,
        "G" => goto_last_line,

        "g" => {
            "g" => goto_first_line,
            "e" => goto_word_end_backward,
            "E" => goto_long_word_end_backward,
        },

        "d" | "x" => delete_selection,
        "D" | "X" => delete_selection_linewise,
        "c" => change_selection,
        "C" => change_selection_linewise,

        "o" => flip_selection,
    })
}
