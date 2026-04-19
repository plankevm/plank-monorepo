# ERC20 Token

A standard ERC20 fungible token implementation. This is the most common smart contract pattern and covers all the foundational concepts needed to write contracts in Plank:

- Contract structure: `init` block (constructor) and `run` block (runtime dispatch)
- Storage: reading and writing persistent state with `sload` / `sstore`
- Function dispatch: matching on selectors to route calls
- ABI decoding: unpacking calldata into typed values
- Events: emitting `Transfer` and `Approval` logs
- Structs: wrapping raw values into meaningful types (e.g., an `addr` type)
- Arithmetic with overflow checks
