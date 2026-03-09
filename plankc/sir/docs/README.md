# Sensei IR (SIR) Docs

SIR is minimal, low-level intermediate representation for targeting the EVM. Its goals are to be
simple, generalizable for any kind of contract and have clear semantics.

The IR is low-level, meaning it does not have high-level abstractions for things like contract
storage, ABI, etc.

SIR is a _functional_ SSA IR, meaning locals are statically assigned once. Intead of phi-nodes data
flow merging is represented via basic block inputs.

## Structure

### Internal Functions

Internal functions allow you to defines procedure that can be reused within the contract. Every
function points to its entry basic block. Internal functions must have disjoint control flow graphs.

Internal functions can accept and return and arbitrary number of values explicitly.

Internal functions denote the entry points to the contract. A contract MUST HAVE have an `init`
entry point. Entry points must not have any input parameters. The init entrypoint denotes the code
that is executed at init-time and returns the deployed, runtime code. A contract SHOULD HAVE a
runtime entry point (`main`) that denotes the code that will be deployed. It can be ommitted if the
init entrypoint intends to not return code or return custom bytecode.

Recursion (aka cycles in the call graph) is **not supported** at this time as it would heavily
complicate reasonable code generation.

Internal function inputs are defined by the entry basic block input count. Outputs are defined by
the outputs of the basic blocks that end with `iret`.


### Basic Blocks

Basic blocks are the discrete units that group operations in the IR. A basic block will not directly have
control flow in the middle. Basic blocks may contain calls to internal functions which have their
own control flow.

Basic blocks have inputs and outputs denoting values that flow into other basic blocks. Basic block
inputs are the equivalent of phi nodes, assuming the value of the corresponding output from the
incoming basic block.

Example: Simple for-loop that increments `i` until `10` in IR:

```
// BB called `for_loop_entry`, no inputs 1 output.
for_loop_entry -> start_i {
    start_i = const 0
    // Essentially `i := start_i`
    => @for_loop
}

// BB called `for_loop_body`, 1 input 1 output.
for_loop_body i -> next_i {
    c1 = const 1
    next_i = add i c1
    c10 = const 10
    next_i_lt_10 = lt next_i c10
    // Jumping back to self is equivalent to `i := next_i`.
    => next_i_lt_10 ? @for_loop_body : @for_loop_end
}

// BB
for_loop_end end_i { }
```

### Locals

Locals are the variables of the IR holding 256-bit EVM words. The IR has no explicit types. Some types can
be inferred by optimization passes based on the used operations.

### Operations

Similar to Yul available operations are most of the EVM opcodes exposed as function calls.
Exceptions include:
- termination instructions: `STOP`, `RETURN`, `REVERT`, `SELFDESTRUCT`, `INVALID` (except as the last operation in a basic block)
- stack manipulation instructions: `POP`, `DUP<x>`, `PUSH<x>`, `SWAP<x>`
- control flow instructions: `JUMP`, `JUMPI`

Besides low-level EVM operations some low-level abstractions are also provided as operations:
- Memory allocation & management: `malloc(size) -> ptr`, `mallocany(size) -> ptr`, `freeptr() -> ptr`, `salloc(const_size) -> ptr`, `sallocany(const_size) -> ptr`
- Memory read/write: `mstore<x>(ptr, value)`, `mload<x>(ptr) -> value` (beyond the EVM's 256-bit MSTORE & MLOAD and 8-bit MSTORE8, bit sizes 8-256 are supported in increments of 8)
- Internal Function Call: `icall(func, ...param locals) -> ...result locals`
- Code offset to data object: `data_offset(data_obj) -> offset`
- Assign/store constant: `const(const) -> const`, `large_const(const) -> const` (semantically equivalent, `const` just has a more
  efficient representation in the IR)
- No op: `noop()`
- Copy local: `copy(value) -> value`
- Runtime start offset: `runtime_start_offset() -> offset` (yields the code offset where the runtime
  code starts in the init code)
- Initcode end offset: `init_end_offset() -> offset` (the end of the compiled init code, useful for
  retrieving constructor arguments)
- Runtime code length: `runtime_length() -> length` (the length in bytes of the compiled runtime code,
  including bytes objects)

### Control Flow

Control flow occurs between basic blocks within a function. Available control flow primitives are:
- termination (if the basic block ends in a terminating EVM op)
- internal function return
- unconditional goto to another basic block (BB) in the same function
- conditional goto to another BB (branches based on whether value is zero or not)
- switch on a local
