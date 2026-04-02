# Lint Rules

Rules for AI agents to check during code review and implementation across all
projects in this monorepo.

## Idiomatic Control Flow

- **Use return values for check-and-act**: When a method like `.add()` or
  `.insert()` returns whether the element was new, use that return value instead
  of a separate `.contains()` check followed by the mutation. For example,
  prefer `if !set.add(x) { return; }` over
  `if set.contains(x) { return; } set.add(x);`.
