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
// or @custom({ skip })
get fun test_something() {}
```

#### fail_with — Requires termination with the given exit code

```tolk
@custom({ fail_with: 10 })
get fun test_something() {}
```

Alternatively, you can use `expectToEndWithExitCode(code)` function to set the expected exit code conditionally in the test.

```tolk
import "lib/testing/expect"

get fun test_something() {
    if (something) {
        expectToEndWithExitCode(10);
    }

    throw 20; // Test will end with exit code 20 and be considered as failed
}
```

#### gas_limit — Set the gas limit for the test

```tolk
@custom({ gas_limit: 10000 })
get fun test_something() {}
```

#### todo — Mark test as TODO

Such test is not executed, but marked as TODO.

```tolk
@custom("todo")
get fun test_something() {}

@custom({ todo: "Implement this feature later" })
get fun test_something() {}
```

You can combine multiple annotations:

```tolk
@custom({ fail_with: 10, gas_limit: 5000 })
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
