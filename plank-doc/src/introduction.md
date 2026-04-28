# Introduction

Plank is an EVM-native programming language for writing smart contracts, giving you direct access to every opcode as a builtin function, without inline assembly blocks, or separate syntax. At the same time, it provides modern language features like compile-time evaluation (comptime), generics, structs, modules, and first-class functions. By combining low-level control with high-level ergonomics, Plank lets you write expressive code that still compiles to efficient bytecode.

Under the hood, Plank compiles to EVM bytecode via Sensei IR (SIR), a language-agnostic intermediate representation designed for fast compilation and EVM-specific optimizations.

## Why Plank

The EVM tooling stack has lagged behind. "Stack too deep" errors, slow compile times, and unreliable editor support shouldn't exist in 2026. Plank aims to raise the bar on expressivity, compilation speed, and developer experience.

Comptime is Plank's superpower that makes this design possible. Instead of relying on separate macro systems, template metaprogramming, or external code generation, Plank uses a single unified mechanism for generics, type introspection, constant computation, and metaprogramming. Inspired by Zig, this keeps the language simple while still enabling powerful abstractions, without adding runtime cost.

## Status

Plank is still in its early stages: the compiler, standard library, and language features are actively evolving. If you run into bugs, have ideas for improvement, or want to get involved, open an issue on [GitHub](https://github.com/plankevm/plank-monorepo) or reach out on [X](https://x.com/plankevm).
