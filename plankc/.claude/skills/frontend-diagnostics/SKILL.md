---
name: frontend-diagnostics
description: Mandatory use when adding, modifying, or reviewing diagnostic emission in any frontend pipeline stage. Use when touching emit_ methods, Session::emit_diagnostic, or the Diagnostic/Cause/Claim builder API.
---

# Frontend Diagnostics

## Overview

Every frontend pipeline stage emits diagnostics through dedicated `emit_`
methods that take minimal domain inputs, build a `Diagnostic` value, and forward
it to `Session`. The pipeline never constructs diagnostics inline at the call
site.

## Architecture

```
Call site (business logic)
    |  calls emit_foo(index, span, ...)
    v
emit_ method (on pipeline struct)
    |  resolves names/spans via self
    |  builds Diagnostic via builder API
    v
Session::emit_diagnostic(diagnostic)
```

- **Call sites stay clean.** Pass only what they naturally hold (an index, a
  span). No `Diagnostic` construction, no `format!`.
- **emit_ methods own the translation.** Resolve indices to names, look up
  related spans, assemble the `Diagnostic`.
- **Context is opaque.** The pipeline only knows `Session::emit_diagnostic`.

## Quick Reference

### Diagnostic Builder

| Method | Purpose |
|--------|---------|
| `Diagnostic::error(title)` | Start an error diagnostic |
| `Diagnostic::warning(title)` | Start a warning diagnostic |
| `.primary(source_id, span, label)` | Convenience: pushes a lone `Cause` with a single primary annotation |
| `.element(impl Into<Element>)` | Push any element (Cause, Origin, Message) to primary group |
| `.note(message)` | Push a note message element |
| `.help(message)` | Push a help message element |
| `.add_claim(claim)` | Push a secondary group (renders as separate titled section) |

### Cause Builder

Groups annotations for a single source file. Converts `Into<Element>`.

| Method | Purpose |
|--------|---------|
| `Cause::new(source_id)` | Start a cause for a source file |
| `.primary(span, label)` | Add a primary annotation (`AnnotationKind::Primary`) |
| `.secondary(span, label)` | Add a context annotation (`AnnotationKind::Context`) |
| `.span(span, kind)` | Add an unlabeled annotation with explicit `AnnotationKind` |

### Claim Builder

Creates a secondary group rendered with `secondary_title`.

| Method | Purpose |
|--------|---------|
| `Claim::new(level, title)` | Start a claim (e.g. `Level::Note, "entry file"`) |
| `.element(impl Into<Element>)` | Push element (Cause, Origin, etc.) |
| `.primary(source_id, span, label)` | Convenience: lone Cause with primary annotation |
| `.note(message)` / `.help(message)` | Push message elements |

### Element Variants

| Variant | Purpose |
|---------|---------|
| `Element::Cause { source, annotations }` | Annotated source snippet (built via `Cause`) |
| `Element::Origin { path: SourceId }` | Bare file reference (`::: path`) |
| `Element::Message { level, text }` | Footer-style message |
| `Element::Suggestion { source, patches }` | Patch-based code suggestion |

## The emit_ Method Pattern

### Anti-pattern: inline Diagnostic at call site

```rust
// BAD: formatting scattered in business logic
let name = self.resolve_name(ident_idx);
self.session.borrow_mut().emit_diagnostic(
    Diagnostic::error(format!("cannot find `{name}` in this scope"))
        .primary(self.source_id, self.span_of(ident_idx), "not found in this scope")
);
```

### Correct: dedicated emit_ method

```rust
// Call site: passes only what it naturally has
self.emit_unresolved_ident(ident_idx);
```

```rust
// In a diagnostics impl block (typically diagnostics.rs):
impl BlockLowerer<'_> {
    fn emit_unresolved_ident(&self, ident: IdentIdx) {
        let name = self.resolve_name(ident);
        let span = self.span_of(ident);
        self.emit_diagnostic(
            Diagnostic::error(format!("cannot find `{name}` in this scope"))
                .primary(self.source_id, span, "not found in this scope")
        );
    }

    fn emit_diagnostic(&self, diagnostic: Diagnostic) {
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }
}
```

Rules:
- `emit_` methods live on the pipeline's main struct, grouped in a dedicated
  `diagnostics.rs` file or impl block
- Parameters are minimal domain types (indices, spans), never pre-built strings
  or Diagnostic fragments
- The method uses `self` to resolve anything it needs
- A single `emit_diagnostic` helper method forwards to the session

## Annotation Grouping

Each `.primary()` on `Diagnostic` creates a **separate** `Cause`. To group
multiple annotations into one snippet, build a `Cause` explicitly:

```rust
// Two annotations in ONE snippet (same source):
Diagnostic::error("variable not declared mutable")
    .element(
        Cause::new(self.source_id)
            .primary(assign_span, "assignment to immutable variable")
            .secondary(decl_span, "declared here")
    )
    .help("consider declaring it with `let mut`")

// Two annotations in SEPARATE snippets (different sources):
Diagnostic::error("unresolved import")
    .primary(self.source_id, span, "not found in target module")
    .add_claim(
        Claim::new(Level::Note, "target module")
            .element(Element::Origin { path: target_source })
    )
```

When two `SourceId`s might or might not be the same, branch explicitly:

```rust
if self.source_id == prev_source_id {
    diagnostic = diagnostic.element(
        Cause::new(self.source_id)
            .primary(span, "conflicting import")
            .secondary(prev_span, prev_label),
    );
} else {
    diagnostic = diagnostic
        .primary(self.source_id, span, "conflicting import")
        .element(Cause::new(prev_source_id).secondary(prev_span, prev_label));
}
```

## Borrow Strategy

**Default — `&mut Session` directly:** when emit_ methods can take `&mut self`.

```rust
struct Pipeline {
    session: Session,
}
impl Pipeline {
    fn emit_some_error(&mut self, span: SourceSpan) {
        self.session.emit_diagnostic(
            Diagnostic::error("...").primary(self.source_id, span, "...")
        );
    }
}
```

**`RefCell` — when emit_ needs `&self`:** when business logic holds shared
borrows into the struct during emission (common in tree-walking lowerers):

```rust
struct BlockLowerer<'a> {
    session: RefCell<&'a mut Session>,
}
impl BlockLowerer<'_> {
    fn emit_diagnostic(&self, diagnostic: Diagnostic) {
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }
}
```

**Decision rule:** methods take `&self` and read multiple fields while emitting?
Use `RefCell`. Otherwise `&mut Session`.

## Common Mistakes

| Mistake | Why it's wrong | Fix |
|---------|---------------|-----|
| Building `Diagnostic` at the call site | Scatters formatting, noisy call sites, inconsistent messages | Dedicated `emit_` method |
| Passing `String` args to `emit_` methods | Forces caller to resolve names, duplicates lookup logic | Pass indices/spans, let `emit_` resolve via `self` |
| `panic!`/`todo!` instead of emitting | Kills compilation on first error, no useful output | Emit diagnostic and return a poison/sentinel value |
| Bare `Element::Origin` without context | Lone file path with no explanation | Wrap in a `Claim` with descriptive title |
| Same-source annotations in separate `Cause`s | Renders as two disconnected snippet blocks | Group in one `Cause` with `.primary()` + `.secondary()` |
