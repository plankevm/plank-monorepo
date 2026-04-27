# Merkle Airdrop

A token airdrop contract where eligible recipients are encoded in a Merkle tree. Users claim their allocation by submitting a Merkle proof, which the contract verifies on-chain before marking the claim. Token distribution is not handled.

This example demonstrates:

- Iterating over calldata using a `while` loop to process proof elements
- Handling variable-length calldata for runtime-sized proofs

```plank
{{#include ../../../plankc/plank-diff-tests/src/examples/merkle_airdrop.plk}}
```

## Claiming

The `claim` function performs the core verification logic:

1. Checks if the caller has already claimed by reading their storage slot
2. Reads the address, amount, and Merkle proof from calldata
3. Computes the leaf hash from the address and amount
4. Iterates over the proof, sorting each node pair before hashing to compute the next layer
5. Compares the final computed root with the stored root; if valid, marks the claim and emits an event

The proof is processed in a `while` loop, reading each element from calldata by its offset:

```plank
let mut i = 0;
while i < proof_length {
    let proof_element = @evm_calldataload(offset + 32 + i * 32);
    ...
    i = i + 1;
}
```

## Leaf Computation

The leaf is computed by packing the address (20 bytes) and amount (32 bytes) into a manually allocated buffer:

```plank
let buf = @malloc_uninit(64);
@mstore32(buf, address);
@mstore32(buf +% 32, amount);
let mut node = @evm_keccak256(buf +% 12, 52);
```

