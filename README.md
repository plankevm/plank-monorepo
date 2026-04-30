# Plank Monorepo

Welcome To Plank's home!

To install the compiler run:

```bash
curl -L install.plankevm.org | bash && plankup
```

Or, with Nix flakes enabled, run Plank directly from the flake:

```bash
nix run github:plankevm/plank-monorepo#plank -- --help
```

The Nix package includes the compiler, standard library, and local docs. The wrapped `plank` binary sets `PLANK_DIR` to the package output so `plank build` can find `stdlib/` and `plank doc` can find `share/doc/`.

If you'd like to contribute please read the [Contributor Guidelines](./CONTRIBUTING.md).

## Contents
- `std/`: Plank Standard Library
- `plankc/`: Plank Compiler
    - [`frontend/`](./plankc/frontend/): Plank Frontend
    - [`sir/`](./plankc/sir/): Sensei IR (Plank's Low-Level IR & Backend)
    - [Plank Examples](`./plankc/plank-diff-tests/src/examples/`)
- `plank-doc/`: Plank Docs
- `plank-tree-sitter/`: Tree sitter grammar & parser for the plank language

## Donating

If you'd like to support use you can donate at
[Our Project Page on Giveth](https://qf.giveth.io/project/plankevm). Thanks to
the currently running QF round and matching pool even a small donation has a
massive impact, thank you.
