import "message"
import "../testing/assert"
import "../tlb/either"
import "../vm/vm"

struct OutList {
    action: OutAction
}

struct (0x0ec3c86d) OutActionSendMessage {
    next: Cell<OutList>
    mode: uint8,
    out_msg: cell
}

fun OutActionSendMessage.loadMessage<Msg>(self): MessageRelaxed<Msg> {
    return (self.out_msg as Cell<MessageRelaxed<Msg>>).load({
        assertEndAfterReading: false,
    });
}

fun OutActionSendMessage.loadBody<Msg>(self): Msg {
    return self.loadMessage<Msg>().loadBody()
}

fun OutActionSendMessage.loadGenericMessage(self): MessageRelaxedGeneric {
    return (self.out_msg as Cell<MessageRelaxedGeneric>).load({
        assertEndAfterReading: false,
    });
}

struct (0xad4de08e) OutActionSetCode {
    next: Cell<OutList>
    new_code: cell
}

struct (0x36e6b809) OutActionReserveCurrency {
    next: Cell<OutList>
    mode: uint8,
    currency: CurrencyCollection
}

struct (0x26fa1dd4) OutActionChangeLibrary {
    next: Cell<OutList>
    mode: uint7,
    libref: LibRef
}

fun OutActionChangeLibrary.toTuple(self) {
    var tuple = createEmptyTuple();
    tuple.push(self.next);
    tuple.push(self.mode);
    if (self.libref is LibRefHash) {
        tuple.push(0);
        tuple.push(self.libref.lib_hash);
    } else {
        tuple.push(1);
        tuple.push(self.libref.library);
    }
    return tuple
}

struct (0b0) LibRefHash {
    lib_hash: bits256
}

struct (0b1) LibRefRef {
    library: cell
}

type LibRef = LibRefHash | LibRefRef

type OutAction = OutActionSendMessage | OutActionSetCode | OutActionReserveCurrency | OutActionChangeLibrary

fun OutActionSendMessage.fromTuple(data: tuple): OutActionSendMessage asm "3 UNTUPLE"
fun OutActionSetCode.fromTuple(data: tuple): OutActionSetCode asm "2 UNTUPLE"
fun OutActionReserveCurrency.fromTuple(data: tuple): OutActionReserveCurrency asm "4 UNTUPLE"
fun OutActionChangeLibrary.fromTuple(data: tuple): OutActionChangeLibrary asm "4 UNTUPLE"
fun LibRefHash.fromTuple(data: tuple): LibRefHash asm "1 UNTUPLE"
fun LibRefRef.fromTuple(data: tuple): LibRefRef asm "1 UNTUPLE"
fun LibRef.fromTuple(data: tuple): LibRef {
    val tag = data.get<int>(0);
    if (tag == 0) {
        return LibRefHash.fromTuple(data)
    } else if (tag == 1) {
        return LibRefRef.fromTuple(data)
    } else {
        throw 0xFF // unknown libref type
    }
}
fun OutAction.fromTuple(data: tuple): OutAction {
    if (data.size() == 2) {
        return OutActionSetCode.fromTuple(data)
    } else if (data.size() == 3) {
        return OutActionSendMessage.fromTuple(data)
    } else if (data.size() == 4) {
        val third = data.get<int>(2);
        if (third == 0 || third == 1) {
            return OutActionChangeLibrary.fromTuple(data)
        }
        return OutActionReserveCurrency.fromTuple(data)
    } else {
        Assert.fail("Unknown out action")
    }
}

fun OutAction.kind(self): slice {
    match (self) {
        OutActionSendMessage => {
            return "send-message"
        }
        OutActionSetCode => {
            return "set-code"
        }
        OutActionReserveCurrency => {
            return "reserve-currency"
        }
        OutActionChangeLibrary => {
            return "change-library"
        }
    }

    return "unknown-action";
}

type OutActionList = tuple

fun OutActionList.at(self, i: int): OutAction {
    val raw_action = self.get<tuple>(i);
    return OutAction.fromTuple(raw_action);
}

fun OutActionList.getSendMessageAt(self, i: int): OutActionSendMessage? {
    val action = self.at(i);
    if (action is OutActionSendMessage) {
        return action
    }
    return null
}

fun OutActionList.getSendMessageBodyAt<Msg>(self, i: int): Msg? {
    val action = self.at(i);
    if (action is OutActionSendMessage) {
        val msg = (action.out_msg as Cell<MessageRelaxed<Msg>>).load({
            assertEndAfterReading: false,
        });

        if (msg.body is EitherLeft<Msg>) {
            return msg.body.value
        }
        if (msg.body is EitherRight<Cell<Msg>>) {
            return msg.body.value.load({
                assertEndAfterReading: false
            })
        }
        return null
    }
    return null
}

fun OutMessage.outActions(self, mode: int): OutActionList {
    // val prev_c5 = vm.getC5();
    self.send(mode);
    val list = vm.outActions();
    // vm.setC5(prev_c5);
    return list;
}

fun vm.outActions(): OutActionList {
    return vm.parseOutActions(vm.getC5())
}

fun vm.parseOutActions(cell: cell): OutActionList {
    var result = createEmptyTuple();
    val root = cell as Cell<OutList>;
    var next: Cell<OutList>? = root;

    while (next != null) {
        val loaded = next.load();
        match (loaded.action) {
            OutActionSendMessage => {
                result.push(loaded.action.__toTuple());
                if (loaded.action.next.beginParse().remainingBitsCount() == 0) {
                    next = null
                } else {
                    next = loaded.action.next;
                }
            }
            OutActionSetCode => {
                result.push(loaded.action.__toTuple());
                if (loaded.action.next.beginParse().remainingBitsCount() == 0) {
                    next = null
                } else {
                    next = loaded.action.next;
                }
            }
            OutActionReserveCurrency => {
                result.push(loaded.action.__toTuple());
                if (loaded.action.next.beginParse().remainingBitsCount() == 0) {
                    next = null
                } else {
                    next = loaded.action.next;
                }
            }
            OutActionChangeLibrary => {
                result.push(loaded.action.toTuple());
                if (loaded.action.next.beginParse().remainingBitsCount() == 0) {
                    next = null
                } else {
                    next = loaded.action.next;
                }
            }
        }
    }

    return result;
}
