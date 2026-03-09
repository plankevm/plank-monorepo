# EVM Opcodes and Precompiles Documentation

This document contains all EVM opcodes and precompiled contracts with their inputs, outputs, detailed descriptions, and hardfork introduction information.

## EVM Opcodes

### Arithmetic Operations

#### 0x00 - STOP
- **Hardfork:** Frontier
- **Input:** None
- **Output:** None
- **Description:** Halts execution of the current context. No more operations will be executed and the current call frame will end.

#### 0x01 - ADD
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a + b
- **Description:** Pops two values from the stack, adds them together, and pushes the result back onto the stack. The addition is performed modulo 2^256.

#### 0x02 - MUL
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a * b
- **Description:** Pops two values from the stack, multiplies them, and pushes the result. The multiplication is performed modulo 2^256.

#### 0x03 - SUB
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a - b
- **Description:** Pops two values from the stack (first a, then b), subtracts b from a, and pushes the result. If b > a, the result wraps around modulo 2^256.

#### 0x04 - DIV
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a // b
- **Description:** Integer division operation. Pops two values, divides a by b, and pushes the integer quotient. If b is 0, the result is 0.

#### 0x05 - SDIV
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a // b
- **Description:** Signed integer division operation treating values as two's complement signed 256-bit integers. Division rounds toward zero. If b is 0, the result is 0.

#### 0x06 - MOD
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a % b
- **Description:** Modulo remainder operation. Pops two values and pushes the remainder of a divided by b. If b is 0, the result is 0.

#### 0x07 - SMOD
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a % b
- **Description:** Signed modulo remainder operation. Treats values as two's complement signed integers. The sign of the result matches the sign of a. If b is 0, the result is 0.

#### 0x08 - ADDMOD
- **Hardfork:** Frontier
- **Input:** a | b | N
- **Output:** (a + b) % N
- **Description:** Modular addition. Pops three values, adds a and b, then takes the modulo N of the result. All intermediate calculations are done with arbitrary precision. If N is 0, the result is 0.

#### 0x09 - MULMOD
- **Hardfork:** Frontier
- **Input:** a | b | N
- **Output:** (a * b) % N
- **Description:** Modular multiplication. Pops three values, multiplies a and b, then takes the modulo N. All intermediate calculations are done with arbitrary precision. If N is 0, the result is 0.

#### 0x0a - EXP
- **Hardfork:** Frontier
- **Input:** a | exponent
- **Output:** a ** exponent
- **Description:** Exponential operation. Pops two values and pushes a raised to the power of exponent, modulo 2^256. Gas cost increases with the size of the exponent.

#### 0x0b - SIGNEXTEND
- **Hardfork:** Frontier
- **Input:** b | x
- **Output:** y
- **Description:** Extend the length of a two's complement signed integer. Takes a byte position b and a value x, and sign-extends x from the (b+1)*8-th bit.

### Comparison & Bitwise Logic Operations

#### 0x10 - LT
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a < b
- **Description:** Less-than comparison. Pops two values and pushes 1 if a is less than b, 0 otherwise. Comparison is done on unsigned integers.

#### 0x11 - GT
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a > b
- **Description:** Greater-than comparison. Pops two values and pushes 1 if a is greater than b, 0 otherwise. Comparison is done on unsigned integers.

#### 0x12 - SLT
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a < b
- **Description:** Signed less-than comparison. Treats values as two's complement signed integers. Pushes 1 if a < b, 0 otherwise.

#### 0x13 - SGT
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a > b
- **Description:** Signed greater-than comparison. Treats values as two's complement signed integers. Pushes 1 if a > b, 0 otherwise.

#### 0x14 - EQ
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a == b
- **Description:** Equality comparison. Pops two values and pushes 1 if they are equal, 0 otherwise.

#### 0x15 - ISZERO
- **Hardfork:** Frontier
- **Input:** a
- **Output:** a == 0
- **Description:** Checks if the top stack value is zero. Pushes 1 if the value is 0, pushes 0 otherwise.

#### 0x16 - AND
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a & b
- **Description:** Bitwise AND operation. Pops two values and pushes the result of a bitwise AND operation on every bit.

#### 0x17 - OR
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a | b
- **Description:** Bitwise OR operation. Pops two values and pushes the result of a bitwise OR operation on every bit.

#### 0x18 - XOR
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** a ^ b
- **Description:** Bitwise XOR (exclusive or) operation. Pops two values and pushes the result of a bitwise XOR on every bit.

#### 0x19 - NOT
- **Hardfork:** Frontier
- **Input:** a
- **Output:** ~a
- **Description:** Bitwise NOT operation. Pops one value and pushes the bitwise complement (flips all bits).

#### 0x1a - BYTE
- **Hardfork:** Frontier
- **Input:** i | x
- **Output:** y
- **Description:** Retrieve a single byte from a word. Pops an index i and value x, pushes the i-th byte of x (0-indexed from the left, most significant byte). If i >= 32, returns 0.

#### 0x1b - SHL
- **Hardfork:** Constantinople
- **Input:** shift | value
- **Output:** value << shift
- **Description:** Left shift operation. Shifts value left by shift bits, filling with zeros from the right. If shift >= 256, the result is 0.

#### 0x1c - SHR
- **Hardfork:** Constantinople
- **Input:** shift | value
- **Output:** value >> shift
- **Description:** Logical right shift operation. Shifts value right by shift bits, filling with zeros from the left. If shift >= 256, the result is 0.

#### 0x1d - SAR
- **Hardfork:** Constantinople
- **Input:** shift | value
- **Output:** value >> shift
- **Description:** Arithmetic (signed) right shift operation. Shifts value right by shift bits, filling with the sign bit from the left. Treats value as a two's complement signed integer.

### SHA3

#### 0x20 - KECCAK256 (SHA3)
- **Hardfork:** Frontier
- **Input:** offset | size
- **Output:** hash
- **Description:** Computes the Keccak-256 hash of the data in memory. Pops memory offset and size, reads that data from memory, and pushes the 32-byte hash result. Gas cost includes memory expansion.

### Environmental Information

#### 0x30 - ADDRESS
- **Hardfork:** Frontier
- **Input:** None
- **Output:** address
- **Description:** Pushes the address of the currently executing account (the account whose code is running) onto the stack.

#### 0x31 - BALANCE
- **Hardfork:** Frontier
- **Input:** address
- **Output:** balance
- **Description:** Gets the balance of the specified account in wei. Pops an address and pushes that account's balance. Gas cost increases if the address is "cold" (first access in the transaction).

#### 0x32 - ORIGIN
- **Hardfork:** Frontier
- **Input:** None
- **Output:** address
- **Description:** Pushes the address of the original transaction sender (tx.origin). This remains constant throughout all message calls.

#### 0x33 - CALLER
- **Hardfork:** Frontier
- **Input:** None
- **Output:** address
- **Description:** Pushes the address of the direct caller of the current execution context (msg.sender).

#### 0x34 - CALLVALUE
- **Hardfork:** Frontier
- **Input:** None
- **Output:** value
- **Description:** Pushes the amount of wei sent with the current call (msg.value).

#### 0x35 - CALLDATALOAD
- **Hardfork:** Frontier
- **Input:** i
- **Output:** data[i]
- **Description:** Loads 32 bytes of input data. Pops an offset and pushes 32 bytes from the input data starting at that offset. Pads with zeros if reading past the end.

#### 0x36 - CALLDATASIZE
- **Hardfork:** Frontier
- **Input:** None
- **Output:** size
- **Description:** Pushes the size of the input data in bytes for the current call.

#### 0x37 - CALLDATACOPY
- **Hardfork:** Frontier
- **Input:** destOffset | offset | size
- **Output:** None
- **Description:** Copies input data to memory. Pops memory destination offset, input data offset, and size. Copies that many bytes from input data to memory. Gas includes memory expansion cost.

#### 0x38 - CODESIZE
- **Hardfork:** Frontier
- **Input:** None
- **Output:** size
- **Description:** Pushes the size of the code of the currently executing account in bytes.

#### 0x39 - CODECOPY
- **Hardfork:** Frontier
- **Input:** destOffset | offset | size
- **Output:** None
- **Description:** Copies code to memory. Pops memory destination offset, code offset, and size. Copies that many bytes from the current contract's code to memory. Gas includes memory expansion cost.

#### 0x3a - GASPRICE
- **Hardfork:** Frontier
- **Input:** None
- **Output:** price
- **Description:** Pushes the gas price of the current transaction in wei per gas unit.

#### 0x3b - EXTCODESIZE
- **Hardfork:** Frontier
- **Input:** address
- **Output:** size
- **Description:** Gets the code size of an external account. Pops an address and pushes the size of that account's code in bytes. Returns 0 for non-contract accounts. Gas cost increases if address is "cold".

#### 0x3c - EXTCODECOPY
- **Hardfork:** Frontier
- **Input:** address | destOffset | offset | size
- **Output:** None
- **Description:** Copies an external account's code to memory. Pops address, memory destination, code offset, and size. Gas includes memory expansion and cold address access costs.

#### 0x3d - RETURNDATASIZE
- **Hardfork:** Byzantium
- **Input:** None
- **Output:** size
- **Description:** Pushes the size of the return data from the previous call. Available after CALL, CALLCODE, DELEGATECALL, or STATICCALL.

#### 0x3e - RETURNDATACOPY
- **Hardfork:** Byzantium
- **Input:** destOffset | offset | size
- **Output:** None
- **Description:** Copies return data from the previous call to memory. Reverts if accessing beyond return data bounds. Gas includes memory expansion cost.

#### 0x3f - EXTCODEHASH
- **Hardfork:** Constantinople
- **Input:** address
- **Output:** hash
- **Description:** Gets the keccak256 hash of an account's code. Returns 0 for non-existent accounts and 0xc5d2...c35b for accounts without code. Gas cost increases if address is "cold".

### Block Information

#### 0x40 - BLOCKHASH
- **Hardfork:** Frontier
- **Input:** blockNumber
- **Output:** hash
- **Description:** Gets the hash of one of the 256 most recent complete blocks. Pops a block number and pushes that block's hash. Returns 0 for blocks outside the valid range.

#### 0x41 - COINBASE
- **Hardfork:** Frontier
- **Input:** None
- **Output:** address
- **Description:** Pushes the current block's beneficiary address (the miner/validator who will receive block rewards).

#### 0x42 - TIMESTAMP
- **Hardfork:** Frontier
- **Input:** None
- **Output:** timestamp
- **Description:** Pushes the current block's timestamp as seconds since the Unix epoch.

#### 0x43 - NUMBER
- **Hardfork:** Frontier
- **Input:** None
- **Output:** blockNumber
- **Description:** Pushes the current block number.

#### 0x44 - DIFFICULTY / PREVRANDAO
- **Hardfork:** Frontier / Merge
- **Input:** None
- **Output:** difficulty / prevRandao
- **Description:** Pre-merge: Pushes the current block's difficulty. Post-merge: Pushes the previous block's RANDAO mix (randomness beacon).

#### 0x45 - GASLIMIT
- **Hardfork:** Frontier
- **Input:** None
- **Output:** gasLimit
- **Description:** Pushes the current block's gas limit.

#### 0x46 - CHAINID
- **Hardfork:** Istanbul
- **Input:** None
- **Output:** chainId
- **Description:** Pushes the chain ID (used for replay protection). For example, 1 for Ethereum mainnet.

#### 0x47 - SELFBALANCE
- **Hardfork:** Istanbul
- **Input:** None
- **Output:** balance
- **Description:** Pushes the balance of the currently executing account. More gas-efficient than using BALANCE with ADDRESS.

#### 0x48 - BASEFEE
- **Hardfork:** London
- **Input:** None
- **Output:** baseFee
- **Description:** Pushes the current block's base fee per gas (EIP-1559). This is the minimum fee required for inclusion in the block.

#### 0x49 - BLOBHASH
- **Hardfork:** Cancun
- **Input:** index
- **Output:** versionedHash
- **Description:** Returns the versioned hash of the blob at the given index. Used for blob transactions (EIP-4844).

#### 0x4a - BLOBBASEFEE
- **Hardfork:** Cancun
- **Input:** None
- **Output:** blobBaseFee
- **Description:** Returns the current block's blob base fee. Used for pricing blob data in blob transactions.

### Stack, Memory, Storage and Flow Operations

#### 0x50 - POP
- **Hardfork:** Frontier
- **Input:** y
- **Output:** None
- **Description:** Removes the top item from the stack and discards it.

#### 0x51 - MLOAD
- **Hardfork:** Frontier
- **Input:** offset
- **Output:** value
- **Description:** Loads a 32-byte word from memory. Pops a memory offset and pushes the 32 bytes starting at that offset. Gas includes memory expansion cost.

#### 0x52 - MSTORE
- **Hardfork:** Frontier
- **Input:** offset | value
- **Output:** None
- **Description:** Stores a 32-byte word to memory. Pops memory offset and value, then stores the value at that memory location. Gas includes memory expansion cost.

#### 0x53 - MSTORE8
- **Hardfork:** Frontier
- **Input:** offset | value
- **Output:** None
- **Description:** Stores a single byte to memory. Pops memory offset and value, then stores the least significant byte of the value. Gas includes memory expansion cost.

#### 0x54 - SLOAD
- **Hardfork:** Frontier
- **Input:** key
- **Output:** value
- **Description:** Loads a word from storage. Pops a storage key and pushes the corresponding value. Gas cost depends on whether the slot is "cold" or "warm".

#### 0x55 - SSTORE
- **Hardfork:** Frontier
- **Input:** key | value
- **Output:** None
- **Description:** Stores a word to storage. Pops key and value, then stores the value at that storage slot. Gas cost varies based on the current value, original value, and whether it's a cold or warm access.

#### 0x56 - JUMP
- **Hardfork:** Frontier
- **Input:** counter
- **Output:** None
- **Description:** Alters the program counter to a specific position. Pops a destination and jumps to that position. The destination must be a JUMPDEST instruction.

#### 0x57 - JUMPI
- **Hardfork:** Frontier
- **Input:** counter | b
- **Output:** None
- **Description:** Conditional jump. Pops a destination and a condition. If condition is non-zero, jumps to destination (which must be a JUMPDEST). Otherwise, continues to next instruction.

#### 0x58 - PC
- **Hardfork:** Frontier
- **Input:** None
- **Output:** counter
- **Description:** Pushes the current program counter (the position of the PC instruction itself in the bytecode).

#### 0x59 - MSIZE
- **Hardfork:** Frontier
- **Input:** None
- **Output:** size
- **Description:** Pushes the size of active memory in bytes. This is 32 times the highest memory offset accessed, rounded up.

#### 0x5a - GAS
- **Hardfork:** Frontier
- **Input:** None
- **Output:** gas
- **Description:** Pushes the amount of gas remaining, including the reduction for this instruction.

#### 0x5b - JUMPDEST
- **Hardfork:** Frontier
- **Input:** None
- **Output:** None
- **Description:** Marks a valid destination for JUMP and JUMPI operations. Has no effect on execution flow by itself.

#### 0x5c - TLOAD
- **Hardfork:** Cancun
- **Input:** key
- **Output:** value
- **Description:** Loads a word from transient storage. Similar to SLOAD but for transient storage that only lasts for the duration of the transaction.

#### 0x5d - TSTORE
- **Hardfork:** Cancun
- **Input:** key | value
- **Output:** None
- **Description:** Stores a word to transient storage. Similar to SSTORE but the storage is discarded after the transaction completes.

#### 0x5e - MCOPY
- **Hardfork:** Cancun
- **Input:** destOffset | offset | size
- **Output:** None
- **Description:** Copies memory areas within memory. More efficient than using MLOAD/MSTORE in a loop. Gas includes memory expansion for both source and destination.

### Push Operations

#### 0x5f - PUSH0
- **Hardfork:** Shanghai
- **Input:** None
- **Output:** 0
- **Description:** Pushes the value 0 onto the stack. More gas-efficient than PUSH1 0x00.

#### 0x60 to 0x7f - PUSH2 through PUSH32
- **Hardfork:** Frontier
- **Input:** None
- **Output:** value
- **Description:** Pushes N-byte values onto the stack where N is the number in the opcode name. The bytes follow immediately after the opcode in the bytecode.

### Duplication Operations

#### 0x80 - DUP1
- **Hardfork:** Frontier
- **Input:** value
- **Output:** value | value
- **Description:** Duplicates the 1st stack item (top of the stack).

#### 0x81 to 0x8f - DUP2 through DUP16
- **Hardfork:** Frontier
- **Input:** ... values ...
- **Output:** ... values with Nth item duplicated on top ...
- **Description:** Duplicates the Nth stack item and pushes the duplicate onto the top of the stack.

### Exchange Operations

#### 0x90 - SWAP1
- **Hardfork:** Frontier
- **Input:** a | b
- **Output:** b | a
- **Description:** Exchanges the top two stack items.

#### 0x91 to 0x9f - SWAP2 through SWAP16
- **Hardfork:** Frontier
- **Input:** top | ... | Nth
- **Output:** Nth | ... | top
- **Description:** Exchanges the top stack item with the Nth stack item.

### Logging Operations

#### 0xa0 - LOG0
- **Hardfork:** Frontier
- **Input:** offset | size
- **Output:** None
- **Description:** Appends a log entry with no topics. Logs are accessible to external observers but not to contracts. Gas includes memory expansion and log data costs.

#### 0xa1 - LOG1
- **Hardfork:** Frontier
- **Input:** offset | size | topic
- **Output:** None
- **Description:** Appends a log entry with one topic. Topics can be used to filter logs efficiently.

#### 0xa2 - LOG2
- **Hardfork:** Frontier
- **Input:** offset | size | topic1 | topic2
- **Output:** None
- **Description:** Appends a log entry with two topics.

#### 0xa3 - LOG3
- **Hardfork:** Frontier
- **Input:** offset | size | topic1 | topic2 | topic3
- **Output:** None
- **Description:** Appends a log entry with three topics.

#### 0xa4 - LOG4
- **Hardfork:** Frontier
- **Input:** offset | size | topic1 | topic2 | topic3 | topic4
- **Output:** None
- **Description:** Appends a log entry with four topics (the maximum allowed).

### System Operations

#### 0xf0 - CREATE
- **Hardfork:** Frontier
- **Input:** value | offset | size
- **Output:** address
- **Description:** Creates a new contract. Pops value (wei to send), memory offset and size of initialization code. Pushes the address of the created contract, or 0 on failure. The new address is deterministic based on sender and nonce.

#### 0xf1 - CALL
- **Hardfork:** Frontier
- **Input:** gas | address | value | argsOffset | argsSize | retOffset | retSize
- **Output:** success
- **Description:** Message-call into an account. Executes code of another contract with a new message call. Can transfer value. Returns 1 on success, 0 on failure.

#### 0xf2 - CALLCODE
- **Hardfork:** Frontier
- **Input:** gas | address | value | argsOffset | argsSize | retOffset | retSize
- **Output:** success
- **Description:** Message-call using current account's storage. Executes external code in the context of the current contract. Deprecated in favor of DELEGATECALL.

#### 0xf3 - RETURN
- **Hardfork:** Frontier
- **Input:** offset | size
- **Output:** None
- **Description:** Halts execution and returns data. Pops memory offset and size, then stops execution and returns that data to the caller.

#### 0xf4 - DELEGATECALL
- **Hardfork:** Byzantium
- **Input:** gas | address | argsOffset | argsSize | retOffset | retSize
- **Output:** success
- **Description:** Message-call into account with current account's storage, sender and value. Executes external code in the current contract's context. Used for library calls.

#### 0xf5 - CREATE2
- **Hardfork:** Constantinople
- **Input:** value | offset | size | salt
- **Output:** address
- **Description:** Creates a contract with deterministic address. Uses salt to make the deployment address predictable and independent of future nonce values.

#### 0xfa - STATICCALL
- **Hardfork:** Byzantium
- **Input:** gas | address | argsOffset | argsSize | retOffset | retSize
- **Output:** success
- **Description:** Static message-call into an account. Like CALL but disallows state modifications. Used for read-only external calls.

#### 0xfd - REVERT
- **Hardfork:** Byzantium
- **Input:** offset | size
- **Output:** None
- **Description:** Halts execution, reverts state changes, but returns data and remaining gas. Used for error handling while preserving gas for the caller.

#### 0xfe - INVALID
- **Hardfork:** Homestead
- **Input:** None
- **Output:** None
- **Description:** Designated invalid instruction. Consumes all remaining gas and reverts all state changes. Used to mark invalid opcodes.

#### 0xff - SELFDESTRUCT
- **Hardfork:** Frontier
- **Input:** address
- **Output:** None
- **Description:** Halts execution and marks account for deletion. Sends all ether to the specified address. In post-Cancun, only sends balance but doesn't delete the account in the same transaction.

## Precompiled Contracts

### 0x01 - ecRecover
- **Hardfork:** Frontier
- **Input:** hash (32 bytes) | v (32 bytes) | r (32 bytes) | s (32 bytes)
- **Output:** publicAddress (32 bytes, left-padded with zeros)
- **Description:** Elliptic curve digital signature algorithm (ECDSA) public key recovery function. Recovers the Ethereum address from a signature. Returns zero address on invalid input.

### 0x02 - SHA2-256
- **Hardfork:** Frontier
- **Input:** data (arbitrary length)
- **Output:** hash (32 bytes)
- **Description:** Computes the SHA2-256 hash of the input data. Gas cost scales linearly with input size.

### 0x03 - RIPEMD-160
- **Hardfork:** Frontier
- **Input:** data (arbitrary length)
- **Output:** hash (32 bytes, left-padded with zeros)
- **Description:** Computes the RIPEMD-160 hash of the input data. The 20-byte result is left-padded with 12 zero bytes. Gas cost scales linearly with input size.

### 0x04 - identity
- **Hardfork:** Frontier
- **Input:** data (arbitrary length)
- **Output:** data (same as input)
- **Description:** Returns the input data unchanged. Useful for copying data with predictable gas costs. Gas cost scales linearly with input size.

### 0x05 - modexp
- **Hardfork:** Byzantium
- **Input:** Bsize (32 bytes) | Esize (32 bytes) | Msize (32 bytes) | B | E | M
- **Output:** B^E mod M (padded to Msize bytes)
- **Description:** Arbitrary-precision exponentiation under modulo. Computes B^E mod M where B, E, and M can be arbitrary size. Gas cost depends on input sizes and exponent value.

### 0x06 - ecAdd
- **Hardfork:** Byzantium
- **Input:** x1 (32 bytes) | y1 (32 bytes) | x2 (32 bytes) | y2 (32 bytes)
- **Output:** x (32 bytes) | y (32 bytes)
- **Description:** Point addition on the alt_bn128 elliptic curve. Adds two points and returns the result. Used for zkSNARK verification.

### 0x07 - ecMul
- **Hardfork:** Byzantium
- **Input:** x1 (32 bytes) | y1 (32 bytes) | s (32 bytes)
- **Output:** x (32 bytes) | y (32 bytes)
- **Description:** Scalar multiplication on the alt_bn128 elliptic curve. Multiplies a point by a scalar. Used for zkSNARK verification.

### 0x08 - ecPairing
- **Hardfork:** Byzantium
- **Input:** x1 | y1 | x2 | y2 | ... | xk | yk (k pairs of G1 and G2 points)
- **Output:** success (32 bytes, 0x01 for true, 0x00 for false)
- **Description:** Optimal ate pairing check on alt_bn128 curve. Verifies if the product of pairings equals 1. Essential for zkSNARK verification. Input must be a multiple of 192 bytes.

### 0x09 - blake2f
- **Hardfork:** Istanbul
- **Input:** rounds (4 bytes) | h (64 bytes) | m (128 bytes) | t (16 bytes) | f (1 byte)
- **Output:** h (64 bytes)
- **Description:** BLAKE2b compression function F. Performs the specified number of mixing rounds on the state. Used as a building block for BLAKE2b hash function.

### 0x0a - point evaluation
- **Hardfork:** Cancun
- **Input:** versioned_hash (32 bytes) | z (32 bytes) | y (32 bytes) | commitment (48 bytes) | proof (48 bytes)
- **Output:** FIELD_ELEMENTS_PER_BLOB * 32 bytes
- **Description:** KZG point evaluation precompile. Verifies that p(z) = y given a KZG commitment and proof. Also verifies the commitment matches the versioned hash. Used for EIP-4844 blob transactions.

## Hardfork Timeline

### Frontier (July 30, 2015)
- Initial Ethereum release
- Introduced most basic opcodes (arithmetic, comparison, memory, storage, flow control)
- Basic precompiles: ecRecover, SHA2-256, RIPEMD-160, identity

### Homestead (March 14, 2016)
- Added INVALID (0xfe) opcode

### Byzantium (October 16, 2017)
- Added RETURNDATASIZE (0x3d) and RETURNDATACOPY (0x3e)
- Added REVERT (0xfd)
- Added STATICCALL (0xfa) and DELEGATECALL (0xf4)
- Added modexp (0x05), ecAdd (0x06), ecMul (0x07), ecPairing (0x08) precompiles

### Constantinople (February 28, 2019)
- Added SHL (0x1b), SHR (0x1c), SAR (0x1d) bitwise shift operations
- Added CREATE2 (0xf5) for deterministic contract addresses
- Added EXTCODEHASH (0x3f)

### Istanbul (December 8, 2019)
- Added CHAINID (0x46)
- Added SELFBALANCE (0x47)
- Added blake2f (0x09) precompile

### London (August 5, 2021)
- Added BASEFEE (0x48) for EIP-1559 support

### Merge (September 15, 2022)
- Changed DIFFICULTY (0x44) to PREVRANDAO for proof-of-stake

### Shanghai (April 12, 2023)
- Added PUSH0 (0x5f) for gas optimization

### Cancun (March 13, 2024)
- Added TLOAD (0x5c) and TSTORE (0x5d) for transient storage
- Added MCOPY (0x5e) for efficient memory copying
- Added BLOBHASH (0x49) and BLOBBASEFEE (0x4a) for EIP-4844
- Added point evaluation (0x0a) precompile for KZG commitments

## Notes

1. Gas costs for most operations are not fixed and depend on various factors:
   - Memory expansion: Operations that access memory beyond current bounds incur additional costs
   - Storage operations: Costs vary based on current value, original value, and hot/cold access
   - External calls: Costs include base fee, value transfer, new account creation, and execution

2. Stack depth is limited to 1024 items. Most operations consume items from the top of the stack and push results back.

3. Memory is byte-addressable but only accessible in 32-byte words (except MSTORE8).

4. Storage is word-addressable with 32-byte keys and values.

5. All integer operations are performed modulo 2^256 unless otherwise specified.

6. The precompiled contracts provide cryptographic functions that would be prohibitively expensive to implement in EVM bytecode.
