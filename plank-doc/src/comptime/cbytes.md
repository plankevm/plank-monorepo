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

## Operations

### Slicing And Embedding

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

> [!TIP]
> Use `@concat_bytes((slice,))`  if you wish to ensure that only the bytes contained within your
> slice are added to your contract's bytecode. This works because `@concat_bytes` creates *new*
> strings while slicing merely creates views into other strings.

### Concatenating

You can concatenate multiple `cbytes` and `u256` values together using the `@concat_cbytes` builtin.
`@concat_cbytes` takes as argument a tuple containing the values you want to concatenate. String
literals and hex literals are appended directly since they have type `cbytes`, while `u256` elements
are encoded as 32-byte big-endian byte strings:

```plank
const MESSAGE = @concat_cbytes((
    "count=",
    3,
    hex"0a",
));

// MESSAGE == "count="
//     hex"0000000000000000000000000000000000000000000000000000000000000003"
//     hex"0a"
```

Unlike slicing, `@concat_cbytes` produces an independent `cbytes` value from
the inputs. If you embed the result, only those bytes are embedded, even when
some inputs were slices of larger `cbytes` values:

```plank
import std::error::require;
import std::regions::{embed_as, bytes, code};

const LOOKUP_TABLE =
    hex"0000000000000000000000000000000000000000000000000000000000000000"
    hex"000102030405060708090a0b0c0d0e0f00112233445566778899aabbccddeeff";

// A slice is a view into LOOKUP_TABLE.
const TABLE_WINDOW = @slice_cbytes(LOOKUP_TABLE, 32, 64);

// `@concat_cbytes` copies the visible bytes into an independent cbytes value.
const WINDOW_COPY = @concat_cbytes((TABLE_WINDOW,));

init {
    let table: bytes(code) = embed_as(LOOKUP_TABLE, code);
    let sliced_window: bytes(code) = embed_as(TABLE_WINDOW, code);
    let copied_window: bytes(code) = embed_as(WINDOW_COPY, code);

    // The slice points into the original embedded table.
    require(sliced_window.ptr == table.ptr +% 32);

    // The concat result is embedded independently.
    require(sliced_window.ptr != copied_window.ptr);

    @evm_stop();
}
```

This becomes easier to understand if you imagine the underlying strings. The original literal
is stored in `LOOKUP_TABLE` so that's an original string. The slice stored in `TABLE_WINDOW` is
just a **view** into `LOOKUP_TABLE` while the call to `@concat_bytes` forces the
creation of an independent string which is then stored in `WINDOW_COPY`.

```
LOOKUP_TABLE
        ┊
        ┊
        ▼
┌────────────── 32 bytes ──────────────┬────────────── 32 bytes ──────────────┐
│ 00 00 00 ... 00                      │ 00 01 02 ... ee ff                   │
└──────────────────────────────────────┴──────────────────────────────────────┘
                                       ◄┄┄┄┄┄┄┄┬┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄►
                                               ┊
                                               ┊
TABLE_WINDOW = @slice_cbytes(LOOKUP_TABLE, 32, 64)


┌────────────── 32 bytes ──────────────┐
│ 00 01 02 ... ee ff                   │
└──────────────────────────────────────┘
                  ▲
                  ┊
WINDOW_COPY = @concat_cbytes((TABLE_WINDOW,))
```

### Reading Padded Words

Use `@padded_read_cbytes(bytes, offset)` to read a 32-byte word from a `cbytes` value.

`offset` must be known at compile time and must be in the range `0..=bytes.length`. If fewer than 32 bytes remain after `offset`, the word is padded with zero bytes on the right.

The result is interpreted as a 32-byte big-endian `u256`:

```plank
let char = @padded_read_cbytes("abc", 1);
require(char == 0x6263000000000000000000000000000000000000000000000000000000000000);
let char_byte = char >> (31 * 8);
require(char_byte == 0x62);
```

Reading at the end of a `cbytes` value is valid and returns zero:

```plank
let empty_word = @padded_read_cbytes(hex"010203", 3);
require(empty_word == 0);
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
