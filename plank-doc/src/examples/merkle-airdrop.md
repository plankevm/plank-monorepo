# Merkle Airdrop

A token airdrop contract where eligible recipients are encoded in a Merkle tree. Users claim their allocation by submitting a Merkle proof, which the contract verifies on-chain before releasing tokens.

Beyond the patterns introduced in the ERC20 example, this showcases:

- Loops: iterating over proof elements to walk up the tree
- Hashing: repeated `keccak256` to recompute the root from a leaf and proof
- Calldata handling: reading variable-length proof data
- Tracking claims: using storage to prevent double-claiming
