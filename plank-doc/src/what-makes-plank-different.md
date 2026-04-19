# What Makes Plank Different

Plank is designed to put more power in the hands of smart contract developers. Features like comptime let you extend the language yourself — generating code, precomputing values, introspecting types — without waiting for compiler developers to implement what you need.

This section highlights the key differences you'll encounter coming from Solidity, Vyper, or Huff.

## Contract Structure: `init` and `run`

Plank contracts are organized into two explicit blocks:

- `init` — runs once at deployment. This is where you set up initial storage and return the runtime bytecode.
- `run` — runs on every call to the deployed contract. This is where function dispatch and contract logic live.

There is no implicit constructor or fallback function. What runs at deploy time and what runs at call time is always clear from the structure of your code.

## Comptime

Plank lets you run code at compile time using `comptime` blocks and `comptime` function parameters. Anything that can be computed before deployment doesn't need to cost gas at runtime.

Use `comptime` to precompute values:

```plank
const SECONDS_PER_YEAR = comptime { 365 * 24 * 3600 };
```

Use `comptime` parameters to write generic functions that the compiler specializes per call site:

```plank
const abi_encode = fn(comptime T: type, value: T) memptr {
    // compiler generates encoding logic specific to T
};
```

This is also how the standard library implements ABI encoding, type introspection, and other utilities — by inspecting types at compile time with builtins like `@field_count` and `@field_type`, the compiler generates specialized code with no runtime overhead.

See the [Comptime](./comptime.md) section for a deeper dive.

## Direct EVM Access

Every EVM opcode is available as a builtin function. There's no separate assembly language or inline assembly block — opcodes like `@evm_sload`, `@evm_sstore`, `@evm_caller`, and `@evm_keccak256` are just functions you call directly in your code.

```plank
let owner = @evm_sload(OWNER_SLOT);
if @evm_ne(@evm_caller(), owner) {
    @evm_revert(ptr, 0);
}
```

This means low-level EVM patterns that require assembly blocks in Solidity are just regular Plank code.

## Explicit Memory Management

Plank gives you direct control over memory allocation. There is no hidden free memory pointer — you allocate memory explicitly with `@malloc_uninit` and `@malloc_zeroed`, and you read and write to it with `@mstore32` and `@mload32`.

```plank
let buf = @malloc_zeroed(64);
@mstore32(buf, some_value);
```

You always know where your data lives in memory and how much you've allocated.

## Modules Over Inheritance

Plank has no contract inheritance. Instead, you organize and reuse code through a module system with explicit imports.

```plank
import std::abi::{abi_encode, abi_decode};
import std::math::max;
```

There are no virtual functions, no linearization rules, and no diamond problem. If you want to compose functionality from multiple sources, you import what you need and use it directly.

## ABI Encode and Decode

Plank's standard library provides utilities for ABI encoding and decoding. Thanks to comptime, the compiler inspects your struct fields and generates the right encoding logic automatically — you just pass your type and data.

```plank
import std::abi::{abi_encode, abi_decode};

const Transfer = struct { to: u256, amount: u256 };

let encoded = abi_encode(Transfer, transfer);
let decoded = abi_decode(Transfer, ptr, len);
```

No manual calldata slicing, no hardcoded offsets — define your struct and the standard library handles the rest.

## First-Class Functions

Functions in Plank are values. You can store them in variables, pass them as arguments, and return them from other functions.

This means patterns that require special syntax in Solidity — like access control modifiers — are just functions in Plank:

```plank
const require_owner = fn(action: fn() void) void {
    if @evm_ne(@evm_caller(), OWNER) { revert_empty(); }
    action();
};
```
