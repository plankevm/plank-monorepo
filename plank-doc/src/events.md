# Events

Plank has no `event` keyword. An event is a plain struct; fields wrapped in
`Indexed(T)` become topics, the rest become the data section. The signature and
`topic0` are derived from the type at compile time.

```plank
import std::event::{Indexed, emit};
import std::core::addr::addr;

const Transfer = struct {
    from:   Indexed(addr),
    to:     Indexed(addr),
    amount: u256,
};

emit(Transfer {
    from:   Indexed(addr) { inner: sender },
    to:     Indexed(addr) { inner: recipient },
    amount: value,
});
```

This emits `LOG3` with `topic0 = keccak256("Transfer(address,address,uint256)")`,
computed at comptime and embedded as a single `PUSH32`.

## Signatures

The struct must be bound to a named `const` — the name becomes the event name.
Indexed fields still appear in the signature, in declaration order, exactly as
in Solidity. You can inspect either piece directly:

```plank
const SIG = comptime { event_signature(Transfer) };  // "Transfer(address,address,uint256)"
const T0  = comptime { topic0(Transfer) };           // bytes32
```

## Supported field types

| Plank | Solidity |
|---|---|
| `u256` | `uint256` |
| `UInt(N)` | `uint8` … `uint248` |
| `bool` | `bool` |
| `addr` | `address` |
| `bytes32` | `bytes32` |
| `membytes` | `bytes` |
| `string` | `string` |

Any other field type is a compile error. Structs-as-tuples and arrays are not
yet supported.

Indexed dynamic fields (`bytes`, `string`) become `keccak256` of their
*contents*, per the ABI specification — the value itself is not recoverable
from the log.

## `emit` vs `emit_anonymous`

`emit` reserves `topic0` for the signature hash, so it allows at most **3**
indexed fields and emits `LOG1`–`LOG4`. `emit_anonymous` omits `topic0`,
allowing **4** indexed fields, and emits `LOG0`–`LOG4`. Exceeding either cap is
a compile error.

## Comptime branch quota

Encoding an event resolves its whole shape — which fields are indexed, which
are dynamic, how wide the head is — during compilation, so none of it survives
as runtime code. That comptime work costs branches: roughly a hundred per
`emit`/`emit_anonymous` call, against a default budget of 1000 for the
enclosing evaluation. A contract with a handful of events in one `const` or
entrypoint can exhaust that budget, so both functions call
`@set_eval_branch_quota(10000)` internally.

`@set_eval_branch_quota` is **raise-only** — it takes the maximum of the
requested value and whatever limit is already in effect, so it can never
shrink a budget a caller has chosen. The raised limit is also **scoped to the
enclosing `const` or entrypoint evaluation, not session-wide**: it does not
propagate downward into functions `emit` calls into, and it does not persist
into unrelated `const`/`init`/`run` evaluations elsewhere in the program. If
your own code hits the quota outside of `emit`, you may need to raise it
yourself in that evaluation's scope.

## Why there is no `indexed()` helper

Writing `Indexed(addr) { inner: x }` is verbose, but a helper function costs
36 gas per indexed field per emit site on the SIR backends, which do not inline
single-expression functions. The `Indexed(T)` wrapper, the comptime dispatch
chain that picks `LOG0`–`LOG4`, and `topic0` are all entirely comptime and were
measured byte-identical to hand-written equivalents — the struct literal costs
nothing beyond what a hand-written `@evm_logN` call would.

That is not true of the whole `emit` path, though. The generic ABI data-section
encoder costs real gas because `sir-release` has no inliner: after optimisation,
ERC20's `transfer` is 36330 gas against 35165 for the hand-written version
(+1165), and deployed size is 3579 bytes against 2877 (+702). Do not assume
`emit` is free end-to-end on gas-sensitive contracts — the wrapper and dispatch
are free, but the encoder is not.
