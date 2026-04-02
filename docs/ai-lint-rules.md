# Lint Rules

Rules for AI agents to check during code review and implementation across all
projects in this monorepo.

## Comments

- **No descriptive comments**: Do not add inline comments that describe *what*
  the code does (e.g., `// Parse next element`). Code should be
  self-documenting through descriptive function and variable names.
- **Only explain "why"**: Comments are only acceptable when explaining a
  non-obvious *why* decision.
- **Preserve existing comments**: Never remove existing comments unless they are
  made out of date by new changes.

## Types

- **Type-level over runtime checks**: Always prefer a compile-time, type-level
  check over a runtime check.
- **Precise types**: Use the most specific type available — favor new-typed
  indices, `IndexVec`, and `RelSlice` over raw `u32`/`usize`, `Vec`, and `[T]`.
- **Leverage enums and matches**: Use Rust enums and pattern matching to
  eliminate redundant control flow and nonsensical states.

## Assertions

- **Assert invariants**: Use `assert!`, `assert_eq!`, `.unwrap()`,
  `.expect("reason")` for invariants that **cannot** be enforced via the type
  system.
- **Never for type-enforceable invariants**: If the type system can prevent the
  invalid state, use types instead of assertions.

## Idiomatic Control Flow

- **Use return values for check-and-act**: When a method like `.add()` or
  `.insert()` returns whether the element was new, use that return value instead
  of a separate `.contains()` check followed by the mutation. For example,
  prefer `if !set.add(x) { return; }` over
  `if set.contains(x) { return; } set.add(x);`.

## Dead Code

- **Delete, don't suppress**: Never add `#[allow(dead_code)]`.
- **Delete, don't comment out**: Unused code must be removed, not commented out
  or prefixed with `_` unless required by an external/user-facing API.

## Allocations

- **O(1) allocations**: Functions must make a constant number of heap
  allocations relative to input size.
- **No temp collections**: `.collect()`, `.to_vec()`, and similar are
  anti-patterns when used to create intermediate collections. Use iterator
  chains instead.
- **Borrow before allocating**: When hitting borrow checker conflicts, first try
  narrowing function parameters to specific fields or defining a reusable
  buffer.

## References

- **Borrow over clone**: Pass `&T` or `&[T]` instead of cloning when ownership
  transfer isn't needed.
- **Slice over Vec**: Accept `&[T]` instead of `&Vec<T>` in function parameters.
- **Clone only when necessary**: True ownership transfer, persistence across
  scopes, or `Arc`/`Rc` sharing.
