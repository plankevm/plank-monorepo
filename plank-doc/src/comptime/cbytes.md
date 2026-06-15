# `cbytes`

The `cbytes` type (short for "comptime bytes") is how you define and manipulate
strings of bytes at compile time. They are defined using [string or hex
literals](#syntax).


```plank
const MY_NAME: cbytes = "plank";

const first_8_primes = fn () cbytes {
    hex"020305070d111317"
};

const NOISE = "...123 <hello> $$$";
```

## Purpose

Comptime bytes allow you to extend the power of compile-time
evaluation to things that require strings or other dynamic data, e.g.:
- efficiently defining and embedding the string name of your token contract
- computing selectors or type hashes
- defining comptime arrays
- constructing lookup tables to be used at runtime

## Comptime Only

Any string/bytes literal creates a value of type `cbytes`, which is **comptime
only** and cannot be assigned to or used in runtime contexts.

To use comptime bytes at runtime you must _embed_ it into your contract's bytecode.
This requires an explicit invocation of a function like
`std::regions::embed_as`, which ensures the `cbytes` is added to the bytecode
and returns a `bytes(code)` or `bytes(memory)`.

```plank
import std::regions::{embed_as, bytes, code};

init {
    let name: cbytes = "Plank";

    // ❌ disallowed, as the variable may be mutated based on a runtime condition
    let mut author_name = name;

    // ✅ unlike `cbytes`, `bytes(code)` is runtime compatible as it's simply a
    // struct holding `(code_offset, length)`.
    let mut author_name: bytes(code) = embed_as(name, code);

    @evm_stop();
}
```

### Embedding of Slices

At compile time, you can slice a larger `cbytes` into smaller sub-slices using
the standard library function `std::regions::slice_bytes`:

```plank
import std::regions::{slice_bytes, bytes, code, embed_as};

const ALPHABET = "abcdefghijklmnopqrstuvwxyz";
const BATTLE_SHIP_COLUMNS: cbytes = slice_bytes(ALPHABET, 0, 10);
```

However, if you then proceed to embed that slice, it will embed the entirety of
the original literal (`ALPHABET` in this example)
into your contract. This is because, when you slice a `cbytes`, the
compiler cannot safely determine whether you intend to still use the original by
manipulating the pointer to the sub-slice.

```plank
init {
    // Includes "abcdefghijklmnopqrstuvwxyz" into your contract.
    let columns: bytes(code) = embed_as(BATTLE_SHIP_COLUMNS, code);

    // The pointer derived from `columns` can be used to access any byte
    // embedded from the original `cbytes`.
    let last_letters = @malloc_uninit(16);
    @evm_codecopy(last_letters, columns.ptr +% 10, 16);

    // last_letters holds "klmnopqrstuvwxyz"

    @evm_stop();
}
```

## Syntax

Each `cbytes` literal can be written as an arbitrary number of _segments_ that
are then concatenated together:

```plank
const BYTES1 = "My name "
    "is " /* some comment */ "Plank";
```

There are two kinds of valid bytes segments:
1. String segment: `"(char | single_char_escape | hex_escape)+"` e.g. `"\x73\x6f\x6c\x63 is very \"fast\""`
    - `char`: any printable ASCII character
    - `single_char_escape`: `\0`, `\n`, `\r`, `\t`, `\\`, `\"`
    - `hex_escape`: `\x[0-9A-Fa-f]{2}` e.g. `\x00`, `\x3f`, `\x67`
2. Hex segment: `hex"([0-9A-Fa-f]{2})+"` e.g. `hex"19010300aa00bb3f"`

These can be mixed and matched:

```plank
const EIP191_MESSAGE_PREIMAGE =
    "\x19Ethereum Signed Message:\n32"
    hex"aa59f7855fdd733e28fa54de089e2eacfd253bd4f9c6d4b54c6a6fa8a023bc3a";
```


> [!IMPORTANT]
> Concatenation by adjacency only works for direct literals:
> 
> ```plank
> // ❌ syntax error: `BYTES1` is an identifier, not a string segment.
> const BYTES2 = "Yesterday, he said \"" BYTES1 "\".";
> ```
