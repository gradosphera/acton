import "lib/testing/expect"

get fun test_foo() {
    throw 10;
}

get fun test_bar() {
    expect(10).toEqual(10)
}
