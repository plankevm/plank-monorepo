# Comptime

Comptime is Plank's killer feature. It lets you execute code during compilation rather than at runtime, using the same syntax you already know. The result is embedded directly into the final bytecode — no runtime cost, no overhead.

## What is Comptime

You can use comptime in two ways.

A `comptime` block evaluates an expression at compile time:

```plank
const SECONDS_PER_YEAR = comptime { 365 * 24 * 3600 };
```

A `comptime` parameter tells the compiler to specialize a function for each distinct value it's called with:

```plank
const encode = fn(comptime T: type, value: T) memptr {
    // compiler generates code specific to T
};
```

Sometimes you don't even need to write `comptime` — when all values in an expression are known at compile time, the compiler folds the result automatically. For example, `@evm_add(1, 2)` becomes `3` in the bytecode with no runtime cost.

## What You Get From Comptime

### Reduced On-Chain Cost

The EVM charges for computation, not compilation. Anything you move from runtime to compile time is effectively free.

```plank
const INITIAL_SUPPLY = comptime { 1000 * 10**18 * 365 * 24 * 3600 };
```

The compiler evaluates this once and embeds the final value in the bytecode. No runtime multiplication, no gas spent.

### Readability and Auditability

Uniswap V3's `TickMath.sol` contains constants like this:

```
0xfffcb933bd6fad37aa2d162d1a594001
```

This is `sqrt(1.0001)` in Q128 fixed-point math, used in `getSqrtRatioAtTick`. With comptime, you can write:

```plank
const TICK_BASE_INV = comptime { 1 / sqrt(1.0001) };
```

Both produce the same bytecode, but only one makes it clear what the value represents. Auditors can see how a constant is derived, not just its output. See the [Simple Vault](./examples/simple-vault.md) example for a full contract using this approach.

### Generics

With `comptime` parameters, you write a function once and the compiler specializes it for each type it's called with. No runtime type checks, no unused code paths, no abstraction penalty.

```plank
const max = fn(comptime T: type, a: T, b: T) T {
    if @evm_gt(a, b) { a } else { b }
};
```

One implementation, used with any type — the compiler generates optimized bytecode for each.

### Type Introspection

The compiler can inspect types at compile time — what fields a struct has, their types, their count — and generate code from that information. This means you define your types once and let the compiler handle the boilerplate.

For example, the standard library's ABI encoding works this way:

```plank
import std::abi::abi_encode;

const Transfer = struct { to: u256, amount: u256 };

let encoded = abi_encode(Transfer, transfer);
```

If you add a field to `Transfer`, the encoding updates automatically. No manual changes, no forgotten fields. The same approach can be used for ERC712 typehashes, storage slot computation, and event emitters.
