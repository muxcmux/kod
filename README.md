# Kod

Kod is a modal text/code editor for the terminal, heavily inspired by vim/Neovim and helix. The
main goal of this project is to explore and learn about code editors - how they are made, the
patterns, algorithms and data structures they use, etc.

After the initial few months of exploration, I decided to actually start peeking at helix's source
code and I realized just how much I have written that was already in helix, e.g. the entire
rendering process, terminal ui, compositing was almost identical.

From then on, I continued to study helix's codebase for any new feature I wanted in kod. Sometimes
I'd copy verbatim code, other times I would make slight adjustments. Sometimes I'd decide to take a
different approach, e.g. no dynamic lib loading, or implement the thing myself.

## State of the project

I'm using kod as my `EDITOR` for any `git` operations I do from the terminal, but it is under active
development and I want to gradually start adopting it for more code editing tasks.

## Features

This is a high-level, unordered, non-exhaustive list of features I want in kod.

#### Text editing:

* 🟡 Vim motions: The most obvious ones. I try to add more as I go, but full vim parity is not a goal
* 🟢 Load/Save files from disk
* 🟢 Unicode characters
* 🟢 Multiple cursors
* 🟢 Undo/Redo
* 🟢 Select mode (similar to vim visual mode)
* 🟢 Replace mode
* ⚪️ Dot repeat
* 🟡 Registers
* ⚪️ Copy(yank)/paste
* 🟢 Bracketed paste from clipboard
* 🟢 Document search
* ⚪️ Count before a motion e.g. `5dw`
* 🟢 Syntax highlighting
* 🟡 Theme(s)
* ⚪️ Indentation

#### Workspace and code nav:

* 🟢 Split windows
* 🟢 Multiple open buffers/documents
* 🟢 Basic file explorer ala mini.files / yazi
* 🟡 Commands
* ⚪️ Jump lists
* ⚪️ Built-in fuzzy finder ala Telescope (suitable for more than finding files)
* ⚪️ Project-wide search/replace
* ⚪️ Quickfix lists

#### IDE:

* ⚪️ Code-aware text objects, e.g. "dif" (delete inside function)
* ⚪️ LSP diagnostics, goto definition/impl, actions, hover, etc.
* 🟡 Support for multiple languages
* ⚪️ Snippets
* ⚪️ Mouse
* ⚪️ Git gutter highlights
* ⚪️ Autosuggest (very low on the list)

## Running kod

To build kod you need Rust and a C/C++ compiler and build tools

* Install [Rust](https://www.rust-lang.org/tools/install)

Then do one of these things:

* `$ cargo run` to run kod in dev mode
* `$ cargo run --release` to compile an optimised build
* `$ cargo install --path .` to install kod on your system

Similar to helix, kod will download and compile a bunch of tree-sitter language grammars the first
time it is built. Unlike helix, it statically links the grammars and doesn't require runtime files.

Kod currently does not have any means of loading configuration, keybinds, or themes at runtime.
