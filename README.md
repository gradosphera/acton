<img width="150px" src="docs/img/logo.png">

# Acton

Blazingly fast ~~shit~~ toolkit for TON application development written in
Rust.

## Building

Clone TON monorepo fork:

```
git clone https://github.com/i582/ton/tree/pmakhnev/acton
```

Build and copy artifacts to `./objs`:

```
sh assembly/native/build-macos-static.sh -a && cp ./artifacts/libemulator.a ./artifacts/libtolk.a ../acton/objs
```

Run Rust compilation:

```
cargo build --bin acton
```

In release mode:

```
cargo build --bin acton --release
```

## Run

```
target/debug/acton test foo_test.tolk
# or target/release/acton test foo_test.tolk
```

## Testing

See [Documentation](https://i582.github.io/acton/test-runner/tests/your-first-unit-test-in-tolk/).

## Scripts

Execute Tolk scripts:

```
acton script script.tolk
```

Execute standalone Tolk scripts with a `main()` function:

```tolk
import "lib/io"

fun main() {
    println("Hello, World!");
    println("This is a Tolk script!");
}
```

Scripts exit with the same exit code as the `main()` function finishes.

## Compilation

Compile Tolk files to TVM bytecode:

```
acton compile file.tolk
```

The command outputs the compiled bytecode in base64 format and its hash.

## Development

Run integration tests:

```
cargo test --test integration_test
```

Run debugger tests:

```
cargo test --test debug_test -- --test-threads 1 
```
