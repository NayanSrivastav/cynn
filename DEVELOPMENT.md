# Development Guide: cynn

This guide provides technical instructions for developing, testing, and profiling `cynn`.

---

## 1. Local Environment Setup

Ensure you have the Rust toolchain installed:

```bash
# Install Rust via rustup if not already present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add formatting and linting components
rustup component add rustfmt clippy
```

---

## 2. Running Locally as a Git Subcommand or Standalone CLI

`cynn` can be invoked directly as `cynn` or as a Git subcommand `git cynn`. Git automatically detects any executable in your `$PATH` named `git-<name>` and allows invoking it as `git <name>`.

To test `cynn` / `git cynn` against local repos while developing:

```bash
# In the cynn directory:
cargo build

# Create symlinks to cargo's debug binary in your local bin or path
ln -sf $(pwd)/target/debug/cynn ~/.cargo/bin/cynn
ln -sf $(pwd)/target/debug/git-cynn ~/.cargo/bin/git-cynn

# Now run it anywhere:
cynn src/main.rs:1
# or
git cynn src/main.rs:1
```

---

## 3. Testing Strategy

### Unit Tests
Unit tests are co-located in `src/` modules:
```bash
cargo test --lib
```

### Integration Tests with Temporary Git Fixtures
Integration tests in `tests/` spin up programmatic Git repositories using the `tempfile` crate. They simulate real-world developer scenarios:
* Repositories with multi-line squash merges.
* Repositories with automated Prettier/Black formatting passes.
* Repositories where files were renamed or split into multiple modules.

Run integration tests:
```bash
cargo test --test genesis_test
```

---

## 4. Benchmarking & Performance Budgets

`cynn` has a strict performance budget: **<300ms** for line queries in repositories with up to 100,000 commits.

We use [`hyperfine`](https://github.com/sharkdp/hyperfine) for benchmarking:

```bash
# Compare cynn with raw git blame
hyperfine --warmup 3 \
  'cynn src/main.rs:50' \
  'git blame -L 50,50 src/main.rs'
```

---

## 5. Pre-Commit Checklist

Always run this one-liner before submitting code:

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings && cargo test
```
