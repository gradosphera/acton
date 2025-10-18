import "lib/io"
import "lib/testing/expect"

get fun test_foo() {
    println(1);
    expect(10).toEqual(20)
}

@custom("skip")
get fun test_skip() {
    expect(10).toEqual(20)
}

get fun test_bar() {
    expect(10).toEqual(10)
}
