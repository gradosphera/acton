const ASSERTION_FAILED = 567

struct Assert {}

fun Assert.equal<V1, V2>(left: V1, right: V2, message: slice = "", location: slice = "") {
    if (!assert_equal(left.__toTuple(), right.__toTuple(), message, location)) {
        throw ASSERTION_FAILED;
    }
}

fun Assert.notEqual<V1, V2>(left: V1, right: V2, message: slice = "", location: slice = "") {
    if (assert_equal(left.__toTuple(), right.__toTuple(), message, location)) {
        throw ASSERTION_FAILED;
    }
}

fun assert_equal<V1, V2>(left: V1, right: V2, message: slice = "", location: slice = ""): bool asm "4 EXTCALL"

struct Expectation<T> {
    private value: T
    private location: slice = ""
}

fun Expectation<T>.create(value: T, location: slice = "") {
    return Expectation<T> { value, location }
}

fun Expectation<T>.toEqual<U>(self, expected: U) {
    Assert.equal(self.value, expected, "except(actual).toEqual(expected)", self.location)
}

fun Expectation<T>.toNotEqual<U>(self, expected: U) {
    Assert.notEqual(self.value, expected, "except(actual).toNotEqual(expected)", self.location)
}

fun Expectation<T>.isNull(self) {
    Assert.equal(self.value, null, "except(actual).isNull()", self.location)
}

fun Expectation<T>.isNotNull(self) {
    Assert.notEqual(self.value, null, "except(actual).isNotNull()", self.location)
}

fun expect<T>(value: T, location: slice = "") {
    return Expectation<T>.create(value, location)
}
