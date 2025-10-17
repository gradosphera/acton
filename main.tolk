import "lib/expect"
import "lib/fs"
import "lib/io"

fun register_test(name: slice): void asm "5 EXTCALL"

fun build(path: slice): cell asm "6 EXTCALL"
fun sendMessage(msg: cell, mode: int): Cell<Transaction>? asm "7 EXTCALL"

struct Data {
    value: int32
    is_set: bool
}

get fun `test-first`() {
    expect(Data{
        value: 0, is_set: false
    }).toEqual(Data{
        value: 0, is_set: true
    });
}

get fun `test-second`() {
    Data.fromCell(contract.getData());

    // throw 9;
}

get fun `test-third`() {
    // ...
}

fun run_test(cb: () -> void) {
    cb()
}

struct (0b00) Uninitialized
struct (0b01) Frozen
struct (0b10) Active
struct (0b11) NonExisting

type AccountStatus = Uninitialized | Frozen | Active | NonExisting

struct Transaction {
    tag: int4 // 0x07
    address: uint256
    lt: uint64
    prevTransactionHash: uint256
    prevTransactionLt: uint64
    now: uint32
    outMessagesCount: uint15
    oldStatus: AccountStatus
    endStatus: AccountStatus
}

fun OutMessage.sendSync(self, sendMode: int): Cell<Transaction>? {
    return sendMessage(self.messageCell, sendMode)
}

fun main() {
    val cell = build("1.tolk");
    val address = AutoDeployAddress {
        workchain: 0,
        stateInit: ContractState {
            code: cell,
            data: createEmptyCell(),
        }
    };

    val msg = createMessage({
        bounce: false,
        value: ton("0.1"),
        dest: address,
        body: createEmptyCell(),
    });

    val res = msg.sendSync(SEND_MODE_REGULAR);
    val tx = res!.load({assertEndAfterReading: false});
    println(tx.__toTuple());


    // println(cell);
    // println(address.buildAddress().endCell());

    // val ignore = fs.read_file("hello.txt");
    // if (ignore == null) {
    //     eprintln("Cannot read file");
    //     return
    // }
    // println(ignore);
    //
    // run_test(test_first);
    // register_test("test-first");
    // register_test("test-second");
    // register_test("test-third");

    // expect(10).toEqual(30);
    expect(null).isNotNull();
    expect(Data{
        value: 0, is_set: false
    }).toEqual(Data{
        value: 0, is_set: true
    });

    Assert.equal(10, 20);
    Assert.equal(Data{
        value: 0, is_set: false
    }, Data{
        value: 0, is_set: false
    });
    Assert.equal(Data{
        value: 1, is_set: false
    }, Data{
        value: 0, is_set: false
    });

}
