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

The EVM charges for computation, not compilation. Anything you move from runtime to compile time is effectively free. The `SECONDS_PER_YEAR` example above evaluates entirely at comptime and the result is directly embedded in the bytecode without executing the multiplication on chain.

### Readability and Auditability

If you see `7919` in a codebase, do you know it's the 1000th prime? With comptime, you don't need to guess, you can write `const P_1000 = comptime { nth_prime(1000) };` instead. Both produce the same bytecode, but only one makes it clear what the value represents.

### Zero-Cost Generics

`comptime` parameters allow the compiler to specialize functions per type. No runtime type checks, no unused code paths. One definition, used with any type - the compiler generates optimized code for each.

```plank
const max = fn(comptime T: type, a: T, b: T) T {
    if a > b { a } else { b }
};
```

### Compile-Time Introspection

At `comptime`, everything is a value, including struct types. This lets you define and use functions that perform introspection on them, such as inspecting the number and types of fields.

This enables code generation at compile time, eliminating boilerplate and allowing functions to adapt to the exact structure they operate on.

For example, this is how ABI encoding is handled through the standard library:

```plank
const Transfer = struct { to: u256, amount: u256 };
let encoded = abi_encode(Transfer, transfer);
```

As long as your struct is defined, this call never changes. Add a new field to `Transfer`, and the encoding updates automatically - no manual fixes, no risk of missing fields.

The same pattern can be extended to things like ERC712 type hashes, storage layout, and event encoding: define the type once, and let the compiler keep everything in sync.
