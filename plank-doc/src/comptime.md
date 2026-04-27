# Comptime

Comptime lets you execute code during compilation rather than at runtime, using the same syntax you already know. The result is embedded directly into the final bytecode with no runtime cost.

You can use comptime in two ways:

- A `comptime` block evaluates an expression at compile time

```plank
const SECONDS_PER_YEAR = comptime { 365 * 24 * 3600 };
```

- A `comptime` parameter tells the compiler to specialize a function for each distinct value it is called with:

```plank
const DOUBLE = fn(comptime x: u256) u256 {
    x * 2
};
```

In many cases, compile-time evaluation happens implicitly. When all values in an expression are known at compile time, the compiler evaluates it automatically. For example, `1 + 2` becomes `3` in the bytecode with no runtime cost.

## What You Get From Comptime

### Reduced On-Chain Cost

The EVM charges for computation, not compilation. Anything you move from runtime to compile time is effectively free. The `SECONDS_PER_YEAR` example above evaluates entirely at comptime and the result is directly embedded in the bytecode without executing  the multiplication on chain.

### Readability and Auditability

You might have seen `0xfffcb933bd6fad37aa2d162d1a594001` used in Uniswap V3's `TickMath.sol` - how do you remember it's basically `sqrt(1.0001)`? With comptime, you don't need to! You can write directly: `const TICK_BASE_INV = comptime { 1 / sqrt(1.0001) };`. Both produce the same bytecode, but only one makes it clear what the value represents.

### Zero-Cost Generics

`comptime` parameters allow the compiler to specialize functions per type. No runtime type checks, no unused code paths, no abstraction penalty. One definition, used with any type - the compiler generates optimized code for each.

```plank
const max = fn(comptime T: type, a: T, b: T) T {
    if a > b { a } else { b }
};
```

### Compile-Time Introspection

Types can be inspected at compile time to generate code automatically and eliminate boilerplate.

For example, to ABI encode, you just call the standard library:
```plank
const Transfer = struct { to: u256, amount: u256 };
let encoded = abi_encode(Transfer, transfer);
```

As long as your struct is defined, this call never changes. Add a new field to Transfer, and the encoding updates automatically - no manual fixes, no risk of missing fields.

The same pattern extends to ERC712 type hashes, storage layout, and event encoding: define the type once, and let the compiler keep everything in sync.
