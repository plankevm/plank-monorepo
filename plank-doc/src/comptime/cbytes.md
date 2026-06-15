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

Comptime bytes allow you to extend the power of ahead-of-time, compile-time
evaluation to things that require strings or other dynamic data, e.g.:
- efficiently defining and embedding the string name of your token contract
- computing selectors or type hashes
- defining comptime arrays
- constructing lookup tables to be used at compile time.

## Comptime Only

Any string/bytes literal creates a value of type `cbytes`, which is **comptime
only** and cannot be assigned to or used in runtime contexts. Comptime bytes can
be used at runtime by first _embedding them_ into the runtime. The language
requires you to do this explicitly because it includes the bytes in your
contract's binary, increasing its size:

```plank
import std::regions::*;

init {
    let name: cbytes = "Philogy";

    // ❌ disallowed, as the variable may be mutated based on a runtime condition
    let mut author_name = name;

    // ✅ unlike `cbytes`, `bytes(code)` is a pointer into your contract's data
    // which can be manipulated at runtime.
    let mut author_name: bytes(code) = embed_as(name, code);

    @evm_stop();
}
```

### Embedding of Slices

At compile time you can slice a larger `cbytes` into smaller sub-slices:

```plank
import std::regions::*;

const ALPHABET = "abcdefghijklmnopqrstuvwxyz";
const BATTLE_SHIP_COLUMNS: cbytes = slice_bytes(ALPHABET, 0, 10);
```

However, if you then proceed to embed that slice:

```plank
init {
    let columns: bytes(memory) = embed_as(BATTLE_SHIP_COLUMNS, memory);

    // `use(columns)`;

    @evm_stop();
}
```

It will embed the entirety of the original literal (`ALPHABET` in this example)
into your contract. This is because, when you slice a `cbytes` and use it, the
compiler cannot safely determine whether you intended the original full range to
be valid or not.

## Syntax

Each `cbytes` literal can be written as an arbitrary number of _segments_ that
are then concatenated together. Note that concatenation by adjacency only works
for direct literals:

```plank
// ✅ all 3 segments are direct segments and are only separated by whitespace and comments
const BYTES1 = "My name "
    "is " /* some comment */ "Philippe";

// ❌ syntax error: `BYTES1` is an identifier, not a string segment.
const BYTES2 = "Yesterday, he said \"" BYTES1 ".";
```

There are two kinds of valid bytes segments:
1. String: `"(char | single_char_escape | hex_escape)+"` e.g. `"\x73\x6f\x6c\x63 is very \"fast\""`
    - `char`: any printable ASCII character
    - `single_char_escape`: `\0`, `\n`, `\r`, `\t`, `\\`, `\"`
    - `hex_escape`: `\x[0-9A-Fa-f]{2}` e.g. `\x00`, `\x3f`, `\x67`
2. Hex: `hex"([0-9A-Fa-f]{2})+"`

These can be mixed and matched:

```plank
const EIP191_MESSAGE_PREIMAGE =
    "\x19Ethereum Signed Message:\n32"
    hex"aa59f7855fdd733e28fa54de089e2eacfd253bd4f9c6d4b54c6a6fa8a023bc3a";
```


