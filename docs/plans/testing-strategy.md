# Testing Strategy: Types + Tests

## Philosophy

**Types and tests are complementary, not competing.**

- Types eliminate entire classes of bugs at compile time
- Tests verify business logic and edge cases at runtime
- The richer the type system, the less "type agreement" testing needed

## What Types Handle (No Tests Needed)

- Null/undefined errors → `Option<T>`, `Result<T, E>`
- Type mismatches between functions → compiler enforces
- Structural errors → missing fields, wrong variants caught at compile time
- "Does it even make sense?" → illegal states unrepresentable

## What Tests Handle

- **Business logic correctness** — does the algorithm do what we intend?
- **Edge cases** — boundary conditions, empty inputs, malformed data
- **Property invariants** — laws that should hold for all inputs
- **Integration behavior** — do components work together with real I/O?

## When Tests Enter the Picture

```
┌─────────────────────────────────────────────────────┐
│  Type Definition Phase (Pass 1-4)                   │
│  - Define vocabulary, relationships, shape          │
│  - No tests yet — types ARE the specification       │
│  - cargo check is the feedback loop                 │
└─────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────┐
│  Implementation Phase                                │
│  - Fill in function bodies                          │
│  - Tests start HERE                                 │
│  - Write test → implement → pass → refactor         │
└─────────────────────────────────────────────────────┘
```

## Test Types by Layer

| Layer | Purity | Test Approach |
|-------|--------|---------------|
| Pattern detection | Pure | Unit tests + property tests |
| Hashing | Pure (read effect) | Unit tests with temp files |
| Scanner | I/O effect | Integration tests |
| Reporting | Pure | Unit tests (snapshot style) |
| Quarantine | I/O effect | Integration tests with temp dirs |
| CLI | Glue | End-to-end tests |

## Property-Based Testing

For pure functions, prefer property tests over example tests where possible:

```rust
// Example test: one case
#[test]
fn test_derive_original_copy() {
    assert_eq!(derive_original("foo Copy.txt"), "foo.txt");
}

// Property test: all cases
#[test]
fn prop_roundtrip() {
    // For any valid filename, deriving original then re-adding
    // conflict suffix should be consistent
    proptest!(|(name in "[a-z]+\\.[a-z]+")| {
        let conflict = add_conflict_suffix(&name);
        let original = derive_original(&conflict);
        assert_eq!(original, name);
    });
}
```

## Test Location

Following Rust convention:
- Unit tests: in the same file as the code (`#[cfg(test)] mod tests`)
- Integration tests: in `tests/` directory
- Property tests: alongside unit tests, using `proptest` crate

## Summary

1. **Types first** — define the domain, let compiler check structure
2. **Tests at implementation** — verify logic when filling in function bodies
3. **Pure functions** → property tests preferred
4. **I/O functions** → integration tests with real (temp) filesystems
5. **No redundant tests** — don't test what the compiler already guarantees
