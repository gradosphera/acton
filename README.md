# Acton

<img align="right" src="docs/public/logo.png" height="150px" alt="Acton logo" />

Acton is an all-in-one TON smart contract development toolkit written in Rust.
It combines project scaffolding, build, testing, scripting, wallet and network
operations, verification, linting, formatting, and low-level VM tooling in one
CLI.

Documentation: https://i582.github.io/acton/docs/welcome

<br clear="right" />

## Why Acton

- Single CLI for the full contract lifecycle: create, build, test, deploy,
  verify.
- Native speed (Rust-based toolchain and test runtime).
- Tolk-first workflow with built-in wrappers, testing utilities, and scripts.
- Local development node with faucet, forking, snapshots, and persistence.

### Build from source

Acton links static TON artifacts (`libemulator.a`, `libtolk.a`) from the
`i582/ton` fork branch `pmakhnev/acton`.

```bash
# 1) clone repositories
git clone https://github.com/i582/acton.git
git clone --branch pmakhnev/acton https://github.com/i582/ton.git ton-repo

# 2) build TON static artifacts (example for Linux)
cd ton-repo
./assembly/native/build-ubuntu-static.sh -a -c
cd ..

# 3) copy artifacts into Acton
mkdir -p acton/objs
cp ton-repo/artifacts/libemulator.a acton/objs/
cp ton-repo/artifacts/libtolk.a acton/objs/

# 4) build Acton
cd acton
cargo build
./target/debug/acton --help
```

## Run

```
target/debug/acton test foo.test.tolk
# or target/release/acton test foo.test.tolk
```

## Documentation

See [Documentation](https://i582.github.io/acton/docs/welcome/).

## Development

### Prerequisites

To run tests and contribute to Acton, you'll need to install the following
dependencies:

1. **just**: Command runner used for all development tasks.
   ```bash
   cargo install just
   ```
2. **cargo-nextest**: Modern test runner (highly recommended for faster and
   better test output).
   ```bash
   cargo install cargo-nextest
   ```
3. **bun**: Required for building the Acton Test UI.
   ```bash
   curl -fsSL https://bun.sh/install | bash
   ```
4. **cargo-llvm-cov**: For test coverage reports (optional).
   ```bash
   cargo install cargo-llvm-cov
   rustup component add llvm-tools-preview
   ```
5. **System Dependencies**:
  - **macOS**: `brew install libsodium libmicrohttpd pkg-config graphviz`
  - **Linux**:
    `sudo apt install libsodium-dev libmicrohttpd-dev pkg-config graphviz`

### Running Tests

Run all tests (automatically uses `nextest` if available):

```bash
just test
```

Update test snapshots:

```bash
just test-update
```

Run specific test suites:

```bash
# Integration tests
cargo test --test integration_test

# Debugger tests (must run sequentially)
cargo test --test debug_test -- --test-threads 1
```

To preserve test artifacts:

```
DISABLE_TMP_DIR_CLEANUP_IN_TESTS=1 just test
```

See also: [justfile](justfile) for all available commands.
