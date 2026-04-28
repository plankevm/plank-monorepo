# Getting Started

## Installation

Install Plank using `plankup`:

```bash
curl -L https://install.plankevm.org | bash
```

This installs the `plankup` tool, which manages your Plank installation. It downloads the latest binary to `~/.plank/bin/`, installs local documentation, and can optionally configure syntax highlighting for VS Code, Cursor, and VSCodium.

To update Plank to the latest version, run:

```bash
plankup
```

### Other Editors

**Neovim**

For Neovim check out the [`plank.nvim` extension](https://github.com/plankevm/plank.nvim).

**Zed**

For Zed check out the [`zed-plank` extension](https://github.com/plankevm/plank-monorepo/tree/main/plank-zed).

**Other Editors**

For additional editor support, a [tree-sitter grammar](https://github.com/plankevm/plank-monorepo/tree/main/plank-tree-sitter)
is available. You can find a description of Plank's at grammar in the
[monorepo](https://github.com/plankevm/plank-monorepo/blob/main/plankc/docs/Grammar.md)
if you wish to add support for another editor.

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

Out of the box Plank contracts are very bare bones, giving you access to two entry points: `init`
and `run`. `init` becomes your contract's initcode and runs once at deployment, while `run` is the
entrypoint to your runtime logic. Note nothing is implicit, by default you'll need to use something like
`std::constructor::return_runtime` in your `init` to ensure your contract's
runtime code is set to `run`.

In the above example, when the `init` block runs, it reads the initial magic number from the arguments, stores it, and returns the runtime bytecode. The `run` block executes on every call: it extracts the function selector from the first 4 bytes of calldata and executes the `get()` method if the selector matches `GET_SELECTOR`; otherwise, it reverts.

Compile it:

```bash
plank build magic_number.plk
```

## Browsing the Documentation

Plank also installs the documentation locally. Open it in your browser with:

```bash
plank doc
```

To jump directly to a specific topic:

```bash
plank doc comptime
```

Alternatively feed it to your LLM by pointing it to the `~/.plank/share/doc/src/` folder which contains the docs in their original markdown form.

