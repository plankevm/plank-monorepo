# ERC20 Token

A standard ERC20 implementation covering the core patterns used in Plank:

- Contract structure: `init` (deployment) and `run` (runtime)
- Direct EVM access via opcode builtins (`@evm_sload`, `@evm_sstore`, `@evm_log3`, etc.)
- Explicit memory management with `@malloc_uninit` and `@mstore32`

```plank
{{#include ../../../plankc/plank-diff-tests/src/examples/erc20.plk}}
```

## Imports

The contract uses a few standard library utilities:

- `std::storage::map_slot_hash` - computes mapping storage slots using `keccak256(key, base_slot)`
- `std::constructor::return_runtime` - returns `runtime` bytecode from `init`
- `std::abi::abi_encode` - encodes values into ABI format
- `std::membytes::{membytes, membytes_from_ptr}` - utilities for working with raw memory slices

## Constants

Storage slots and function selectors are defined as constants at the top of the file. There is no compiler-managed storage layout yet, so storage slots are defined explicitly and mapping slots are derived using `map_slot_hash`.

Function selectors and event topics follow standard EVM conventions.

## The `init` Block

The `init` block runs once at deployment. It sets the total supply in slot 0, credits the deployer's balance, emits a `Transfer` event from address zero to the deployer, and returns the runtime bytecode via `return_runtime()`.

## The `run` Block and Dispatch

Every call to the deployed contract enters the `run` block. It extracts the 4-byte selector from the first calldata word and matches it against known selectors using an `if` / `else if` chain. Unrecognized selectors revert.

## Reading and Writing Storage

Reading a balance is a single `@evm_sload` call with a slot derived from `map_slot_hash(address, BALANCE_SLOT_BASE)`. Writing the balance is a `@evm_sstore` with the same slot.

## Events

Events are emitted using `@evm_log3` (three indexed topics plus data):

```plank
@evm_log3(buf, 32, TRANSFER_TOPIC, from, to);
```

## ABI Encoding

```plank
let ptr = @malloc_uninit(10);
@mstore10(ptr, 0x506c616e6b546f6b656e);
let encoded = abi_encode(membytes, membytes_from_ptr(ptr, 10));
@evm_return(encoded.ptr, encoded.len);
```

This pattern writes raw data into memory, wraps it as `membytes`, and passes it to `abi_encode` to produce the output returned to the caller.
