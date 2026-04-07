# Plank (plankevm)

Syntax highlighting for [Plank](https://github.com/plankevm/plank-monorepo), a programming language for EVM smart contract development.

## Features

- Syntax highlighting for `.plk` files
- Keywords, types, literals, operators, comments
- Import path highlighting
- Function call highlighting
- ALL_CAPS constant highlighting
- Auto-closing brackets and comments
- 4-space indentation

## Supported Syntax

- Keywords: `const`, `let`, `mut`, `fn`, `struct`, `init`, `run`, `if`, `else`, `while`, `return`, `import`, `as`, `comptime`, `inline`, `and`, `or`
- Primitive types: `u256`, `bool`, `void`, `memptr`, `type`, `function`, `never`
- Literals: decimal, hex (`0x`), binary (`0b`), booleans (`true`, `false`)
- Comments: line (`//`) and block (`/* */`)

## Installation

### From source

```sh
ln -s /path/to/plank-monorepo/plank-vscode ~/.vscode/extensions/plankevm
```

Then restart VS Code, or open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`) and run "Developer: Reload Window".
