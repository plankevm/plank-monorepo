# Multisig Wallet


A wallet that requires multiple owners to approve a transaction before it can be executed. Owners submit and confirm transactions, and a threshold of approvals is required for execution to happen. Plank does not yet support arrays, so the contract operates on exactly three owners and a threshold of two.

This example demonstrates:

- Decoding calldata into structs with `abi_decode`
- Storing and loading dynamic bytes in storage
- Forwarding calls to other contracts with `@evm_call`

```plank
{{#include ../../../plankc/plank-diff-tests/src/examples/multisig.plk}}
```

## Structs and ABI Encoding/Decoding

The contract defines structs for constructor arguments, submission arguments, and transaction results. These are used with `abi_encode` and `abi_decode` to convert between raw bytes and typed data.

For example, `ConstructorArgs` is used to decode the `init` arguments, i.e., the three owners:

```plank
const ConstructorArgs = struct {
    owner0: u256,
    owner1: u256,
    owner2: u256,
};
```

`abi_decode` takes a type `T` and a `membytes` pointing to the raw input data:

```plank
let args_buf = @malloc_zeroed(96);
@evm_codecopy(args_buf, @init_end_offset(), 96);
let constructor_args = abi_decode(ConstructorArgs, membytes_from_ptr(args_buf, 96));
```

Struct fields are accessed using dot notation:

```plank
@evm_sstore(OWNER0_SLOT, constructor_args.owner0);
```

## Storing Dynamic Bytes

The `@evm_sstore` and `@evm_sload` builtins handle single 32-byte values. For variable-length data, the standard library provides `sstore_bytes` and `sload_bytes`:

```plank
let data = sload_bytes(tx_base_slot +% 2);
```

These handle the underlying storage layout for arbitrary-length byte sequences.

## Executing Transactions

Once enough confirmations are collected, `execute` loads the transaction from storage and forwards it to the target contract using the `@evm_call` builtin, a direct wrapper around the EVM `CALL` opcode:

```plank
let success = @evm_call(@evm_gas(), to, value, data.ptr, data.len, @malloc_uninit(0), 0);
```

All available gas is forwarded along with the value and calldata.
