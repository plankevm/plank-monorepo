# What Makes Plank Different

Plank gives smart contract developers more direct control over how their code is written and executed. It exposes low-level EVM behavior, removes hidden abstractions, and keeps most mechanisms explicit.

This section highlights the key differences you'll encounter coming from Solidity, Vyper, or Huff, and how those differences affect how you write contracts.

## Contract Structure: `init` and `run`

Plank contracts are split into two explicit blocks:
- `init` - deployment logic
- `run` - runtime logic

Unlike Solidity, there's no implicit constructor or fallback. Deployment and execution are separated by design, so it's always clear what runs when.

## Comptime

Plank supports compile-time execution through `comptime`. It enables features like generics, ABI encoding, and type-driven code generation with zero runtime cost.

See [Comptime](./comptime.md) for details.

## Direct EVM Access

Every EVM opcode is exposed as a builtin function. There's no separate assembly or inline assembly: opcodes are called directly in your code via builtins such as `@evm_sload`, `@evm_sstore`, `@evm_caller`, and `@evm_keccak256`.

```plank
let caller = @evm_caller();
let amount = @evm_calldataload(4);
let slot = map_slot_hash(caller, BALANCE_SLOT);
@evm_sstore(slot, amount);
```

Low-level patterns that require assembly in Solidity become regular code in Plank.

## Operator Semantics

Arithmetic operators in Plank are checked by default: `+`, `-`, and `*` revert on overflow and underflow. When wrapping behavior is required, for example for modular arithmetic on storage slots, use the `%`-suffixed variants:

```plank
let sum = a + b;            // reverts on overflow
let slot = base +% offset;  // wraps around
```

Division has two rounding modes, since division can be floored or ceiled:

```plank
7 -/ 2 == 3;   // floor division
7 +/ 2 == 4;   // ceiling division
```

By requiring an explicit choice, Plank avoids the ambiguity present in other languages, where the rounding direction of `/` may depend on context or convention.

## Memory Management

Plank simplifies low-level memory management via `@malloc_uninit` and `@malloc_zeroed` builtins. The compiler controls the final memory layout, tracking allocation lifetimes and optimizing them by removing redundant allocations, merging overlapping ones, reordering them, and promoting dynamic allocations to static slots where possible.

With full visibility into memory usage, the compiler can produce memory layouts that minimize memory footprint, avoid unnecessary reads and writes, and safely spill variables from the EVM stack to memory when needed, eliminating the possibility of ["stack too deep"](https://github.com/argotorg/solidity/issues/14358) errors.

```plank
const map_slot_hash = fn (key: u256, base_slot: u256) u256 {
    let buf = @malloc_uninit(64);
    @mstore32(buf, key);
    @mstore32(buf +% 32, base_slot);
    @evm_keccak256(buf, 64)
};
```

## Modules Over Inheritance

Plank has no contract inheritance. Instead, code is organized through a module system with explicit imports.

```plank
import std::abi::{abi_encode, abi_decode};
import std::math::max;
```

There are no virtual functions, no inheritance linearization, and no diamond problem. Composition is done by importing and using functionality directly.

## ABI Encode and Decode

Plank's standard library provides ABI encoding and decoding utilities. With comptime, the compiler inspects struct fields and generates the correct encoding logic automatically - you only pass the type and data.

```plank
import std::abi::abi_encode;

const Transfer = struct { to: u256, amount: u256 };

let encoded = abi_encode(Transfer, transfer);
@evm_return(encoded.ptr, encoded.len);
```

No manual calldata slicing, no hardcoded offsets - just define your struct, and the standard library handles the rest.

## First-Class Functions

Functions in Plank are values. You can store them in variables, pass them as arguments, and return them from other functions.

Patterns that require special syntax in Solidity, like access control modifiers, are just regular functions in Plank:

```plank
const require_owner = fn(action: fn() void) void {
    if @evm_caller() != OWNER { revert_empty(); }
    action();
};
```
