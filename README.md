# Kod

Kod is a modal text/code editor for the terminal, heavily inspired by vim/Neovim and helix. The
main goal of this project is to explore and learn about code editors - how they are made, the
patterns, algorithms and data structures they utilise, etc.

After the initial few months of exploration, I decided to actually start peeking at helix's source
code and I realized just how much I have written that was already in helix, e.g. the entire
rendering process, terminal ui, compositing was almost identical.

From then on, I continued to study helix's codebase for any new feature I wanted in kod. Sometimes
I'd copy verbatim code, other times I would make slight adjustments. Sometimes I'd decide to take a
different approach, e.g. no dynamic lib loading.

## State of the project

I'm using kod as `EDITOR` for any `git` operations I do from the terminal, but it is under active
development and I want to gradually start adopting it for more code editing tasks.

## Features

This is a high-level, unordered, non-exhaustive list of features I want in kod.

* [-] Vim motions: The most obvious ones. I try to add more as I go, but full vim parity is not a goal
* [x] Load/Save files from disk
* [x] Unicode characters
* [x] Undo/Redo
* [x] Replace mode
* [x] Split windows
* [-] Syntax highlighting (need to adjust themes and automate grammar compilation)
* [-] Search (search working, but no replace at the moment)
* [-] Commands
* [-] Registers
* [-] Count before a motion e.g. `5dw`
* [ ] Jump lists
* [ ] Built-in fuzzy finder ala Telescope (suitable for more than finding files)
* [ ] Project-wide search/replace
* [ ] Basic file explorer ala mini.files
* [ ] LSP diagnostics, goto definition/impl, actions, hover, etc.
* [ ] Quickfix lists
* [ ] Support for multiple languages
* [ ] Visual mode
* [ ] Themes
* [ ] Snippets
* [ ] Mouse
* [ ] Git gutter highlights
* [ ] Autosuggest (very low on the list)

## Build

To build kod you need Rust

* Install [Rust](https://www.rust-lang.org/tools/install)
* cargo run

This will build and run the editor, but it will not not include any grammars.

Kod links with tree-sitter grammars under `./grammars` statically at compile time.

To use a tree-sitter grammar for a language, you have to download and build it.
For example, to include grammar for `rust`, clone it from github:

    $ cd ./grammars && git clone https://github.com/tree-sitter/tree-sitter-rust
    $ cd tree-sitter-rust && make

Until there is an automated system for downloading and compiling grammars, unfortunately
this has to be done manually. Some grammars don't have Makefiles, so you will have to use
`gcc` and `ar` to compile statically linkable libraries. In such cases, I usually just copy
and tweak a Makefile from another grammar.

Grammars need to be in `./grammars/tree-sitter-[grammar-name]`. Directories  under `./grammars`
which don't start with `tree-sitter-` are ignored.
