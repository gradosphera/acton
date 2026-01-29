all: precommit

build:
    cargo build --release

test-unit:
    cargo test --workspace --lib --bins \
        --exclude retrace \
        --exclude ton-executor

test-serial:
    cargo test -p retrace -p ton-executor -- --test-threads 1

test-integration:
    cargo test --test integration_test
    cargo test --test debug_test -- --test-threads 1

test: test-unit test-serial test-integration

test-update:
    SNAPSHOTS=overwrite just test

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-features --all-targets -- -D warnings

check-udeps:
    cargo +nightly udeps --workspace

check: fmt-check clippy test

coverage-setup:
    cargo install cargo-llvm-cov
    rustup component add llvm-tools-preview

coverage:
    cargo llvm-cov --workspace --all-features --all-targets --lcov --output-path lcov.info -- --test-threads 1

coverage-html:
    cargo llvm-cov --workspace --all-features --all-targets --html -- --test-threads 1

coverage-fmt-html:
    cargo llvm-cov -p tolkfmt --all-features --all-targets --html --open

coverage-clean:
    cargo llvm-cov clean

build-ui:
    cd crates/acton-test-ui && bun i && bun run build

precommit: build-ui build check

clean:
    cargo clean
    rm -rf crates/acton-test-ui/dist
