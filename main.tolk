import "lib/fs"
import "lib/io"

fun get_current_weather(where: slice): slice asm "0 EXTCALL"

@pure
@inline
fun Data.toTuple(self, size: int): tuple asm "TUPLEVAR";

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

struct Expectatiom<T> {
    private value: T
}

fun Expectatiom<T>.create(value: T) {
    return Expectatiom<T> { value }
}

fun Expectatiom<T>.toEqual<U>(self, expected: U) {
    Assert.equal(self.value, expected)
}

fun Expectatiom<T>.toNotEqual<U>(self, expected: U) {
    Assert.notEqual(self.value, expected)
}

fun Expectatiom<T>.isNull(self) {
    Assert.equal(self.value, null)
}

fun Expectatiom<T>.isNotNull(self) {
    Assert.notEqual(self.value, null)
}

fun expect<T>(value: T) {
    return Expectatiom<T>.create(value)
}


fun register_test(name: slice): void asm "5 EXTCALL"


struct Data {
    value: int
    is_set: bool
}

get fun `test-first`() {
    expect(Data{ value: 0, is_set: false }).toEqual(Data{ value: 0, is_set: true });
}

get fun `test-second`() {
    throw 9;
}

get fun `test-third`() {
    // ...
}

fun run_test(cb: () -> void) {
    cb()
}

fun main() {
    // val res = get_current_weather("Saint-Petersburg");
    // eprintln(res);

    // val ignore = fs.read_file("hello.txt");
    // if (ignore == null) {
    //     eprintln("Cannot read file");
    //     return
    // }
    // println(ignore);
    //
    // run_test(test_first);
    register_test("test-first");
    register_test("test-second");
    register_test("test-third");

    // expect(10).toEqual(30);
    expect(null).isNotNull();
    expect(Data{ value: 0, is_set: false }).toEqual(Data{ value: 0, is_set: true });

    Assert.equal(10, 20);
    Assert.equal(Data{ value: 0, is_set: false }, Data{ value: 0, is_set: false });
    Assert.equal(Data{ value: 1, is_set: false }, Data{ value: 0, is_set: false });

}
