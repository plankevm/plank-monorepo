# Getting Started

## Installation

Install Plank using the `plankup` installer:

```bash
curl -L https://raw.githubusercontent.com/plankevm/plank-monorepo/main/plankup/install.sh | bash
```

This installs the `plankup` tool, which downloads the latest Plank binary to `~/.plank/bin/`. It also installs the documentation locally and offers to set up syntax highlighting for VS Code, Cursor, and VSCodium.

To update Plank to the latest version, run:

```bash
plankup
```

A [tree-sitter grammar](https://github.com/plankevm/plank-monorepo/tree/main/plank-tree-sitter) is also available for editors like Neovim and Helix.

## Your First Contract

Create a file called `counter.plk`:

```plank
init {
    let runtime = @malloc_uninit(@runtime_length());
    @evm_codecopy(runtime, @runtime_start_offset(), @runtime_length());
    @evm_return(runtime, @runtime_length());
}

run {
    let slot: u256 = 0;
    let count = @evm_sload(slot);
    @evm_sstore(slot, @evm_add(count, 1));
    @evm_stop();
}
```

The `init` block runs at deployment — it copies the runtime bytecode into memory and returns it. The `run` block runs on every call — here it loads a counter from storage, increments it, and stores it back.

Compile it:

```bash
plank build counter.plk
```

## Browsing the Documentation

Plank installs the documentation locally. To open it in your browser:

```bash
plank doc
```

You can also jump directly to a specific topic:

```bash
plank doc comptime
```
