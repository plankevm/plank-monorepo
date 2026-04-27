# Getting Started

## Installation

Install Plank using `plankup`:

```bash
curl -L https://raw.githubusercontent.com/plankevm/plank-monorepo/main/plankup/install.sh | bash
```

This installs the `plankup` tool, which manages your Plank installation. It downloads the latest binary to `~/.plank/bin/`, installs local documentation, and can optionally configure syntax highlighting for VS Code, Cursor, and VSCodium.

To update Plank to the latest version, run:

```bash
plankup
```

For additional editor support, a [tree-sitter grammar](https://github.com/plankevm/plank-monorepo/tree/main/plank-tree-sitter) is available for Neovim, Helix, and other compatible editors.

## Your First Contract

Create a file called `magic_number.plk`:

```plank
import std::constructor::return_runtime;

const MAGIC_NUMBER_SLOT = 0;
const GET_SELECTOR = 0x6d4ce63c;

init {
    let buf = @malloc_zeroed(32);
    @evm_codecopy(buf, @init_end_offset(), 32);
    @evm_sstore(MAGIC_NUMBER_SLOT, @mload32(buf));
    return_runtime();
}

run {
    let selector = @evm_calldataload(0) >> 224;
    if selector == GET_SELECTOR {
        let buf = @malloc_uninit(32);
        @mstore32(buf, @evm_sload(MAGIC_NUMBER_SLOT));
        @evm_return(buf, 32);
    } else {
        @evm_revert(@malloc_uninit(0), 0);
    }
}
```

Plank contracts are explicitly split into two phases: `init` and `run`. `init` runs once at deployment, while `run` handles all subsequent calls at runtime. There is no implicit constructor or fallback behavior.

When the `init` block runs, it reads the initial magic number from the arguments, stores it, and returns the runtime bytecode. The `run` block executes on every call: it extracts the function selector from the first 4 bytes of calldata and executes the `get()` method if the selector matches `GET_SELECTOR`; otherwise, it reverts.

Compile it:

```bash
plank build magic_number.plk
```

## Browsing the Documentation

Plank installs the documentation locally. Open it in your browser with:

```bash
plank doc
```

To jump directly to a specific topic:

```bash
plank doc comptime
```
