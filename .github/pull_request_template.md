## Summary of Changes
Provide a brief summary of what this PR does and why.

## Issue Reference
Fixes #(issue number)

## Type of Change
- [ ] `feat`: New contract functionality or view endpoint
- [ ] `fix`: Bug fix maintaining protocol compatibility
- [ ] `refactor`: Internal code improvement without changing public behavior
- [ ] `perf`: Gas or compute optimization
- [ ] `test`: New characterisation, property, or fuzz tests
- [ ] `docs`: Documentation update in `docs/` or `README.md`
- [ ] `ci`: Workflow or build system modification

## Invariant & Protocol Checklist
- [ ] The three-balance invariant `disbursed <= released <= funded` is strictly preserved.
- [ ] No beneficiary identifiers touch the chain (salted hashes only).
- [ ] Unit tests pass cleanly (`cargo test`).
- [ ] Clippy passes with zero warnings (`cargo clippy --all-targets -- -D warnings`).
- [ ] Formatting conforms to standard (`cargo fmt --all -- --check`).
- [ ] WASM release build succeeds (`cargo build --target wasm32v1-none --release`).
- [ ] Specification (`docs/protocol.md`) updated if public interfaces or errors were changed.
- [ ] No extraneous dependencies added to `Cargo.toml`.
