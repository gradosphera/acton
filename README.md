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

Copy `lib/` with the functions into your project.

To define test, use `get fun test_*() {}` syntax:

```tolk
import "lib/testing/expect"

get fun test_something() {
    expect(5 + 5).toEqual(10)
}

@custom("skip")
get fun test_todo() {
    // ...
}
```

```
acton test foo_test.tolk            # single file
acton test .                        # all test files in dir and subdirs
acton test --filter "test_Foo_.*" . # all test with `test_Foo` prefix
```

### Annotations

#### skip — Skip the test

```tolk
@custom("skip")
get fun test_something() {}
```

#### fail_with — Requires termination with the given exit code

```tolk
@custom("fail_with", 10)
get fun test_something() {}
```

> Note: number literals only for now!

#### gas_limit — Set the gas limit for the test

```tolk
@custom("gas_limit", 10000)
get fun test_something() {}
```

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
