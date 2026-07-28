# ERC20 Token

A standard ERC20 implementation covering the core patterns used in Plank:

- Contract structure: `init` (deployment) and `run` (runtime)
- Direct EVM access via opcode builtins (`@evm_sload`, `@evm_sstore`, `@evm_keccak256`, etc.)
- Explicit memory management with `@malloc_uninit` and `@mstore32`
- Events via `std::event` (`Indexed(T)` fields plus `emit`)

```plank
{{#include ../../../plankc/plank-diff-tests/src/examples/erc20.plk}}
```

## Imports

The contract uses a few standard library utilities:

- `std::storage::map_slot_hash` - computes mapping storage slots using `keccak256(key, base_slot)`
- `std::constructor::return_runtime` - returns `runtime` bytecode from `init`
- `std::abi::abi_encode` - encodes values into ABI format
- `std::membytes::{membytes, membytes_from_ptr}` - utilities for working with raw memory slices
- `std::event::{Indexed, emit}` - Solidity-compatible event definition and emission

## Constants

Storage slots and function selectors are defined as constants at the top of the file. There is no compiler-managed storage layout yet, so storage slots are defined explicitly and mapping slots are derived using `map_slot_hash`.

Function selectors follow standard EVM conventions. Event topics are not constants here — they are derived from the event struct at compile time (see [Events](#events) below).

## The `init` Block

The `init` block runs once at deployment. It sets the total supply in slot 0, credits the deployer's balance, emits a `Transfer` event from address zero to the deployer, and returns the runtime bytecode via `return_runtime()`.

## The `run` Block and Dispatch

Every call to the deployed contract enters the `run` block. It extracts the 4-byte selector from the first calldata word and matches it against known selectors using an `if` / `else if` chain. Unrecognized selectors revert.

## Reading and Writing Storage

Reading a balance is a single `@evm_sload` call with a slot derived from `map_slot_hash(address, BALANCE_SLOT_BASE)`. Writing the balance is a `@evm_sstore` with the same slot.

## Events

Events come from `std::event`. An event is a plain struct; fields wrapped in `Indexed(T)` become topics, the rest become the ABI-encoded data section:

```plank
const Transfer = struct {
    from:   Indexed(addr),
    to:     Indexed(addr),
    amount: u256,
};

emit(Transfer {
    from:   Indexed(addr) { inner: from },
    to:     Indexed(addr) { inner: to },
    amount: amount,
});
```

The event name, the signature `Transfer(address,address,uint256)`, its `topic0` hash, and the choice of `LOG1`–`LOG4` are all resolved at compile time from the struct type — there is no `TRANSFER_TOPIC` constant to keep in sync, and the emitted code is a `LOG3` with a single `PUSH32` topic0. `Approval` works the same way.

See [Events](../events.md) for the full type table, `emit_anonymous`, and the gas characteristics of the data-section encoder.

## ABI Encoding

```plank
let ptr = @malloc_uninit(10);
@mstore10(ptr, 0x506c616e6b546f6b656e);
let encoded = abi_encode(membytes_from_ptr(ptr, 10));
@evm_return(encoded.ptr, encoded.len);
```

This pattern writes raw data into memory, wraps it as `membytes`, and passes it to `abi_encode` to produce the output returned to the caller.
