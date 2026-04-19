# Minimal Proxy (ERC1167)

A factory contract that deploys cheap clones of an existing contract. Each clone delegates all calls to a single implementation contract, drastically reducing deployment costs.

Beyond previous examples, this showcases:

- Raw bytecode construction: assembling the ERC1167 proxy bytecode in memory
- Deploying contracts with `create2` for deterministic addresses
- Direct manipulation of memory and bytecode offsets — the kind of low-level work that's verbose and error-prone in Solidity but natural in Plank
