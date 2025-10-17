const ASSERTION_FAILED = 567

struct Assert {}

fun Assert.equal<V1, V2>(left: V1, right: V2) {
    if (!assert_equal(left.__toTuple(), right.__toTuple())) {
        throw ASSERTION_FAILED;
    }
}

fun Assert.notEqual<V1, V2>(left: V1, right: V2) {
    if (assert_equal(left.__toTuple(), right.__toTuple())) {
        throw ASSERTION_FAILED;
    }
}

fun assert_equal<V1, V2>(left: V1, right: V2): bool asm "4 EXTCALL"

struct Expectation<T> {
    private value: T
}

fun Expectation<T>.create(value: T) {
    return Expectation<T> { value }
}

fun Expectation<T>.toEqual<U>(self, expected: U) {
    Assert.equal(self.value, expected)
}

fun Expectation<T>.toNotEqual<U>(self, expected: U) {
    Assert.notEqual(self.value, expected)
}

fun Expectation<T>.isNull(self) {
    Assert.equal(self.value, null)
}

fun Expectation<T>.isNotNull(self) {
    Assert.notEqual(self.value, null)
}

fun expect<T>(value: T) {
    return Expectation<T>.create(value)
}
