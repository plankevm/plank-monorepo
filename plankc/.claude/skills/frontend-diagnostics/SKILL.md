---
name: frontend-diagnostics
description: Mandatory use when adding, modifying, or reviewing diagnostic emission in any frontend pipeline stage. Use when touching emit_ methods, DiagnosticContext, or the Diagnostic builder API.
---

# Frontend Diagnostics

## Overview

Every frontend pipeline stage emits diagnostics through dedicated `emit_`
methods that take minimal domain inputs, build a `Diagnostic` value, and forward
it to a `DiagnosticContext` sink. The pipeline never constructs diagnostics
inline at the call site.

## Architecture

```
Call site (business logic)
    |  calls emit_foo(index, span, ...)
    v
emit_ method (on pipeline struct)
    |  resolves names/spans via self
    |  builds Diagnostic via builder API
    v
DiagnosticContext::emit(diagnostic)
```

- **Call sites stay clean.** Pass only what they naturally hold (an index, a
  span). No `Diagnostic` construction, no `format!`.
- **emit_ methods own the translation.** Resolve indices to names, look up
  related spans, assemble the `Diagnostic`.
- **Context is opaque.** The pipeline only knows `DiagnosticContext::emit`.

## Quick Reference

### Diagnostic Builder

| Method | Purpose |
|--------|---------|
| `Diagnostic::error(message)` | Start an error diagnostic |
| `Diagnostic::warning(message)` | Start a warning diagnostic |
| `.primary(source_id, span, label)` | Primary annotation (the site the user would edit) |
| `.secondary(source_id, span, label)` | Secondary annotation (context explaining why) |
| `.span(source_id, span, style)` | Unlabelled annotation |
| `.note(message)` | Extra context footer |
| `.help(message)` | Concrete fix suggestion footer |

### Trait

```rust
// plank_diagnostics
pub trait DiagnosticContext {
    fn emit(&mut self, diagnostic: Diagnostic);
}
```

## The emit_ Method Pattern

### Anti-pattern: inline Diagnostic at call site

```rust
// BAD: formatting scattered in business logic
let name = self.resolve_name(ident_idx);
self.diag_ctx.borrow_mut().emit(
    Diagnostic::error(format!("cannot find `{name}` in this scope"))
        .primary(self.source_id, self.span_of(ident_idx), "not found in this scope".into())
);
```

### Correct: dedicated emit_ method

```rust
// Call site: passes only what it naturally has
self.emit_unresolved_ident(ident_idx);
```

```rust
// In a diagnostics impl block (typically diagnostics.rs):
impl<'a, D: DiagnosticContext> BlockLowerer<'a, D> {
    fn emit_unresolved_ident(&self, ident: IdentIdx) {
        let name = self.resolve_name(ident);
        let span = self.span_of(ident);
        self.emit(
            Diagnostic::error(format!("cannot find `{name}` in this scope"))
                .primary(self.source_id, span, "not found in this scope".into())
        );
    }

    /// Shared helper that forwards to the context.
    fn emit(&self, diagnostic: Diagnostic) {
        self.diag_ctx.borrow_mut().emit(diagnostic);
    }
}
```

Rules:
- `emit_` methods live on the pipeline's main struct, grouped in a dedicated
  `diagnostics.rs` file or impl block
- Parameters are minimal domain types (indices, spans), never pre-built strings
  or Diagnostic fragments
- The method uses `self` to resolve anything it needs
- A single `emit` helper method forwards to the context

## Borrow Strategy

**Default — `&mut D` directly:** when emit_ methods can take `&mut self`.

```rust
struct Pipeline<D: DiagnosticContext> {
    diag_ctx: D,
}
impl<D: DiagnosticContext> Pipeline<D> {
    fn emit_some_error(&mut self, span: SourceSpan) {
        self.diag_ctx.emit(
            Diagnostic::error("...".into()).primary(self.source_id, span, "...".into())
        );
    }
}
```

**`RefCell` — when emit_ needs `&self`:** when business logic holds shared
borrows into the struct during emission (common in tree-walking lowerers):

```rust
struct BlockLowerer<'a, D: DiagnosticContext> {
    diag_ctx: RefCell<&'a mut D>,
}
impl<'a, D: DiagnosticContext> BlockLowerer<'a, D> {
    fn emit(&self, diagnostic: Diagnostic) {
        self.diag_ctx.borrow_mut().emit(diagnostic);
    }
}
```

**Decision rule:** methods take `&self` and read multiple fields while emitting?
Use `RefCell`. Otherwise `&mut D`.

## Common Mistakes

| Mistake | Why it's wrong | Fix |
|---------|---------------|-----|
| Building `Diagnostic` at the call site | Scatters formatting, noisy call sites, inconsistent messages | Dedicated `emit_` method |
| Passing `String` args to `emit_` methods | Forces caller to resolve names, duplicates lookup logic | Pass indices/spans, let `emit_` resolve via `self` |
| `panic!`/`todo!` instead of emitting | Kills compilation on first error, no useful output | Emit diagnostic and return a poison/sentinel value |
| Inventing a new trait per pipeline stage | Fragments the system, each stage has its own API | Use the unified `DiagnosticContext` from `plank-diagnostics` |
