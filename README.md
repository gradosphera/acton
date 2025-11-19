<img width="150px" src="docs/img/logo.png">

# Acton

Blazingly fast ~~shit~~ toolkit for TON application development written in
Rust.

## Building

```
cargo build --bin acton
```

In release mode:

```
cargo build --bin acton --profile release
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
