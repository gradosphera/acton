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

```
acton test foo_test.tolk            # single file
acton test .                        # all test files in dir and subdirs
acton test --filter "test_Foo_.*" . # all test with `test_Foo` prefix
```

### Annotations

#### skip — Skip the test

```
@custom("skip")
get fun test_something() {}
```

#### fail_with — Requires termination with the given exit code

```
@custom("fail_with", 10)
get fun test_something() {}
```

> Note: number literals only for now!
