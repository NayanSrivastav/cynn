# Contributing to git-lineage

Thank you for your interest in contributing to **`git-lineage`**!  
We are building a five-star, high-performance developer tool, and we hold our codebase to rigorous engineering standards.

---

## Code of Conduct

We are committed to providing a welcoming, inclusive, and harassment-free environment for all contributors. Please be respectful, constructive, and collaborative.

---

## Getting Started

### Prerequisites
* **Rust 1.78+**: Installed via [`rustup`](https://rustup.rs/).
* **Git 2.30+**: Local Git installation.
* **GitHub CLI (`gh`)**: Optional, for testing remote PR enrichment.

### Clone and Build
```bash
git clone https://github.com/your-org/git-lineage.git
cd git-lineage

# Build in debug mode
cargo build

# Run test suite
cargo test
```

---

## Development Workflow

1. **Check Existing Issues:** Before starting work on a major feature, open or comment on an issue to discuss the design.
2. **Create a Feature Branch:**
   ```bash
   git checkout -b feat/my-new-feature
   ```
3. **Write Tests:** Every PR must include corresponding unit and/or integration tests.
4. **Pass All Quality Gates:**
   Before opening a PR, ensure all checks pass:
   ```bash
   # 1. Format code
   cargo fmt --all -- --check

   # 2. Run linter (Zero-warning policy)
   cargo clippy --all-targets --all-features -- -D warnings

   # 3. Run all tests
   cargo test --all-targets
   ```

---

## Commit Guidelines

We use **Conventional Commits**:
* `feat(core): add rename heuristic to noise filter`
* `fix(tui): resolve keybinding clash in diff viewer`
* `docs(readme): expand AI agent context schema`
* `test(genesis): add integration test for squash commits`

---

## Pull Request Process

1. Open a PR using our [PR Template](.github/PULL_REQUEST_TEMPLATE.md).
2. Ensure CI passes on all target platforms (macOS, Linux).
3. Address review feedback promptly. All PRs require at least one approving review before merging.
