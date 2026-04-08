import "@local-acton/fmt"
import "@local-acton/emulation/network"
import "@local-acton/testing/expect"
import "@local-acton/tlb/maybe"
import "@local-acton/types/message"

struct SingleInt {
    a: int
}

struct DoubleInt {
    a: int = 0
    b: int = 1
}

struct DoubleIntAndNestedSingleInt {
    a: int
    b: int
    nested: SingleInt
}

struct DoubleIntAndNestedDoubleInt {
    a: int = 3
    b: int = 4
    nested: DoubleInt = {}
}

struct VeryNested {
    a: int = 3
    nested: VeryNested1 = {}
}

struct VeryNested1 {
    a: int = 4
    nested: DoubleInt = {}
}

get fun `test-format-struct-with-single-field`() {
    expect(format1("{}", SingleInt { a: 100 })).toEqual("""SingleInt {
    a: 100
}""");
}

get fun `test-format-struct-with-two-fields`() {
    expect(
        format1(
            "{}",
            DoubleInt {
                a: 100,
                b: 200,
            },
        ),
    ).toEqual("""DoubleInt {
    a: 100,
    b: 200
}""");
}

get fun `test-format-struct-with-nested-single-int`() {
    expect(
        format1(
            "{}",
            DoubleIntAndNestedSingleInt {
                a: 100,
                b: 200,
                nested: SingleInt { a: 300 },
            },
        ),
    ).toEqual("""DoubleIntAndNestedSingleInt {
    a: 100,
    b: 200,
    nested: SingleInt {
        a: 300
    }
}""");
}

get fun `test-format-struct-with-nested-double-int`() {
    expect(
        format1("{}", DoubleIntAndNestedDoubleInt {}),
    ).toEqual("""DoubleIntAndNestedDoubleInt {
    a: 3,
    b: 4,
    nested: DoubleInt {
        a: 0,
        b: 1
    }
}""");
}

get fun `test-format-struct-with-very-nested-`() {
    val formatted = format1("{}", VeryNested {});
    expect(
        formatted,
    ).toEqual("""VeryNested {
    a: 3,
    nested: VeryNested1 {
        a: 4,
        nested: DoubleInt {
            a: 0,
            b: 1
        }
    }
}""");
}

get fun `test-format-message`() {
    val src = net.randomAddress("src");
    val dest = net.randomAddress("counter");
    val msg = createMessage({
        bounce: false,
        value: ton("1.9999"),
        dest,
    });
    val result = net.send(src, msg);
    val first = result.at(0);
    val tx = first.tx.load();

    val inMsg = tx.messages.load().inMsg;
    if (inMsg is Value<Cell<MessageRelaxedGeneric>>) {
        var loaded = inMsg.value.load();
        loaded.info.createdLt = 0;
        loaded.info.createdAt = 0;
        expect(
            format1("{}", loaded),
        ).toEqual("""MessageRelaxedGeneric {
    info: IntMsgInfoRelaxed {
        ihrDisabled: true,
        bounce: false,
        bounced: false,
        src: kQC5PHBsPEHzypAUMCGXCD5pNDAA2DozF5CZxqKiB1_dtgb3,
        dest: kQC0663BO7-gCwST4bHolxt85JLRMaFVZgJfblE7oumFduR9 (counter),
        value: CurrencyCollection {
            grams: 1999900000,
            other: map<int32, varuint32> {}
        },
        extraFlags: 0,
        fwdFee: 266669,
        createdLt: 0,
        createdAt: 0
    },
    init: null,
    body: null
}""");
    }
}

get fun `test int toString zero`() {
    expect(0.toString()).toEqual("0");
}

get fun `test int toString single digit`() {
    expect(5.toString()).toEqual("5");
}

get fun `test int toString multi digit`() {
    expect(42.toString()).toEqual("42");
    expect(123456789.toString()).toEqual("123456789");
}

get fun `test int toString negative`() {
    expect((-1).toString()).toEqual("-1");
    expect((-42).toString()).toEqual("-42");
    expect((-123456789).toString()).toEqual("-123456789");
}

get fun `test int toString large number`() {
    expect(1000000000.toString()).toEqual("1000000000");
}

get fun `test storeIntAsString in builder`() {
    var b = beginCell();
    b.storeIntAsString(42);
    expect(b.endCell() as unknown as string).toEqual("42");
}

get fun `test storeIntAsString negative in builder`() {
    var b = beginCell();
    b.storeIntAsString(-99);
    expect(b.endCell() as unknown as string).toEqual("-99");
}

get fun `test-format-multiple-arguments-and-modifiers`() {
    expect(format2("{} {:x}", "hex", 255)).toEqual("hex ff");
    expect(format3("{} {} {:ton}", "value", 7, ton("1.5"))).toEqual("value 7 1.5 TON");
    expect(format4("{{{}}} {} {} {}", 1, 2, 3, 4)).toEqual("{1} 2 3 4");
    expect(
        format5(
            "{} {} {} {} {}",
            1,
            2,
            3,
            4,
            5,
        ),
    ).toEqual("1 2 3 4 5");
}
