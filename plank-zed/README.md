# Plank (plankevm) for Zed

[Zed](https://zed.dev) extension providing language support for [Plank](https://github.com/plankevm/plank-monorepo), a programming language for EVM smart contract development.

It uses the [`plank-tree-sitter`](../plank-tree-sitter) grammar (the `tree-sitter-plank` Rust crate) for parsing and syntax highlighting.

## Features

- Syntax highlighting for `.plk` files via tree-sitter
- Bracket matching and auto-closing
- Line (`//`) and block (`/* */`) comments
- 4-space indentation

## Installation

### From source (dev extension)

In Zed, open the Command Palette (`cmd-shift-p` / `ctrl-shift-p`) and run
`zed: install dev extension`, then select this `plank-zed` directory.

Zed will fetch the tree-sitter grammar from the repository configured in
[`extension.toml`](./extension.toml) (the `plank-tree-sitter` subdirectory of
this monorepo) and compile it to WebAssembly.

## Layout

```
plank-zed/
  extension.toml              # Zed extension manifest (registers the grammar)
  languages/plank/
    config.toml               # Language metadata (name, suffixes, comments)
    highlights.scm            # Tree-sitter syntax highlighting queries
    brackets.scm              # Bracket-matching queries
    indents.scm               # Auto-indentation queries
```
