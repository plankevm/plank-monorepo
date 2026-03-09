# SIR Text Format Specification

## Overview

The Sensei Intermediate Representation (SIR) text format is a human-readable textual representation of EVM bytecode programs. It provides a low-level interface for expressing EVM operations with explicit control flow and basic block structure.

The text format is designed for:
- Manual writing and inspection of IR programs
- Testing and debugging compiler frontends
- Representing optimized intermediate code before final codegen

## EBNF Grammar

This grammar uses regex-style operators: `*` (zero or more), `+` (one or more), `?` (optional), `|` (alternation).


### Program Structure

```ebnf
program          = (function | data_segment)*

function         = "fn" ident ":" newline basic_block+

data_segment     = "data" ident hex_lit newline
```

### Basic Blocks

```ebnf
basic_block      = block_header block_body

block_header     = ident input_list? (thin_arrow output_list)? left_brace newline

input_list       = ident+

output_list      = ident+

block_body       = statement* control_flow? right_brace newline
```

### Statements

```ebnf
statement        = assignment? mnemonic param* newline

assignment       = ident+ equals

mnemonic         = ident

param            = ident        // local variable reference
                 | label        // function/block reference
                 | data_ref     // data segment reference
                 | number       // immediate value
```

### Control Flow

```ebnf
control_flow     = internal_return
                 | unconditional_jump
                 | conditional_jump
                 | switch_stmt

internal_return  = "iret" newline

unconditional_jump = thick_arrow label newline

conditional_jump = thick_arrow ident question label colon label newline

switch_stmt      = "switch" ident left_brace newline
                   switch_case* switch_default?
                   right_brace newline

switch_case      = number thick_arrow label newline

switch_default   = "default" thick_arrow label newline
```

### Lexical Elements

```ebnf
// Comments
line_comment     = "//" (any_char - newline)* newline
block_comment    = "/*" (any_char - "*/")* "*/"
comment          = line_comment | block_comment

// Keywords
keyword          = "fn" | "data" | "switch" | "default" | "iret"

// Identifiers
ident            = (letter | "_") (letter | digit | "_")*
label            = "@" ident
data_ref         = "." ident

// Literals
decimal_lit      = digit+
hex_lit          = "0x" hex_digit+
number           = decimal_lit | hex_lit

// Operators and Delimiters
thin_arrow       = "->"
thick_arrow      = "=>"
equals           = "="
colon            = ":"
question         = "?"
left_brace       = "{"
right_brace      = "}"

// Basic Patterns
letter           = "a".."z" | "A".."Z"
digit            = "0".."9"
hex_digit        = digit | "a".."f" | "A".."F"
newline          = "\n"
whitespace       = " " | "\t" | newline
```

## Operation Mnemonics

Operations are specified by their mnemonic identifier. The parser recognizes 70+ EVM and IR-specific operations:

### Arithmetic Operations
`add`, `mul`, `sub`, `div`, `sdiv`, `mod`, `smod`, `addmod`, `mulmod`, `exp`, `signextend`

### Comparison & Bitwise Operations
`lt`, `gt`, `slt`, `sgt`, `eq`, `iszero`, `and`, `or`, `xor`, `not`, `byte`, `shl`, `shr`, `sar`

### Cryptographic Operations
`keccak256`

### Environment Operations
`address`, `balance`, `origin`, `caller`, `callvalue`, `calldataload`, `calldatasize`, `calldatacopy`, `codesize`, `codecopy`, `gasprice`, `extcodesize`, `extcodecopy`, `returndatasize`, `returndatacopy`, `extcodehash`

### Block Information
`blockhash`, `coinbase`, `timestamp`, `number`, `difficulty`, `gaslimit`, `chainid`, `selfbalance`, `basefee`, `blobhash`, `blobbasefee`

### State Operations
`sload`, `sstore`, `tload`, `tstore`, `gas`

### Logging Operations
`log0`, `log1`, `log2`, `log3`, `log4`

### System Operations
`create`, `create2`, `call`, `callcode`, `delegatecall`, `staticcall`, `return`, `stop`, `revert`, `invalid`, `selfdestruct`

### Memory Operations
`malloc`, `mallocany`, `freeptr`, `salloc`, `sallocany`, `mload<N>`, `mstore<N>`, `mcopy`

Where `<N>` is a bit size from 8-256 in multiples of 8 (e.g., `mload256`, `mstore8`, `mload128`)

### Constant & Data Operations
`const`, `large_const`, `data_offset`, `copy`

### IR Intrinsics
`runtime_start_offset`, `init_end_offset`, `runtime_length`, `icall`, `noop`

## Parser Usage

### Basic Usage

```rust
use sir_parser::parse;

// Parse IR text into an in-memory IR program
let source = r#"
fn main:
    entry {
        c0 = const 0
        c32 = const 32
        a = calldataload c0
        b = calldataload c32
        sum = add a b
        return c0 c32
    }
"#;

match parse(source) {
    Ok(program) => {
        // program is sir_data::Program
        println!("Parsed {} functions", program.functions().len());
    }
    Err(errors) => {
        // Handle parse errors
        for error in errors {
            eprintln!("Parse error: {:?}", error);
        }
    }
}
```

### Error Handling

The parser returns a `Result<Program, Vec<ParseError>>`. Parse errors include:
- Lexical errors (invalid tokens)
- Syntax errors (unexpected tokens, missing delimiters)
- Semantic errors (undefined references, duplicate names)

All errors include span information for precise error reporting.

### AST Structure

The parser internally builds an AST using arena allocation (`bumpalo`), then emits a `sir_data::Program`. The emission phase performs:
- Name resolution (locals, functions, data segments)
- Control flow validation
- Type checking of operation parameters

## Examples

### Simple Function

```
fn main:
    entry {
        c0 = const 0
        c32 = const 32
        a = calldataload c0
        b = calldataload c32
        sum = add a b
        return c0 c32
    }
```

### Function with Multiple Blocks and I/O

```
fn main:
    entry lhs rhs -> sum_out diff_out {
        sum_out = add lhs rhs
        diff_out = sub lhs rhs
        => @ret
    }
    ret sum_in diff_in -> total {
        total = add sum_in diff_in
        iret
    }
```

### Conditional Branch

```
fn conditional:
    entry condition value_a value_b {
        => condition ? @true_case : @false_case
    }
    true_case result_in {
        mstore256 result_in value_a
        => @done
    }
    false_case result_in {
        mstore256 result_in value_b
        => @done
    }
    done {
        stop
    }
```

### Switch Statement

```
fn dispatch:
    entry selector {
        switch selector {
            0 => @case_zero
            1 => @case_one
            2 => @case_two
            default => @fallback
        }
    }
    case_zero {
        // Handle case 0
        stop
    }
    case_one {
        // Handle case 1
        stop
    }
    case_two {
        // Handle case 2
        stop
    }
    fallback {
        // Default handler
        revert
    }
```

### Internal Function Calls

```
fn callee:
    entry x y -> sum diff {
        sum = add x y
        diff = sub x y
        iret
    }

fn caller:
    entry a b {
        result_sum result_diff = icall @callee a b
        // Use result_sum and result_diff
        stop
    }
```

### Data Segments

```
data greeting 0x48656c6c6f2c20576f726c6421
data config 0xdeadbeef

fn main:
    entry {
        offset = data_offset .greeting
        // Use data offset
        stop
    }
```

## Grammar Notes

### Whitespace Handling
- Newlines are significant: they terminate statements and separate blocks
- Spaces and tabs are insignificant within lines
- Empty lines are allowed and ignored

### Parameter Flexibility
- Operations accept any mix of parameter types (locals, labels, data refs, numbers)
- Parameter count and types are validated during emission, not parsing
- This allows the grammar to remain simple while deferring semantic checks

### Block I/O Semantics
- Block inputs represent values flowing into the block (phi nodes or function parameters)
- Block outputs represent values produced by the block before control flow
- Output values become inputs to successor blocks
- The emission phase validates I/O matching between connected blocks

### Control Flow Rules
- Every basic block must end with either:
  - A terminating operation (`stop`, `return`, `revert`, `invalid`, `selfdestruct`)
  - An explicit control flow statement (`iret`, `=>`, switch)
- `iret` is used for internal function returns
- Regular jumps use `=>` with a target label
- Conditional branches use `=> condition ? @target_true : @target_false`
- Switch statements provide multi-way branches based on a value
