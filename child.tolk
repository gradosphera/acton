import "lib/io"

struct Storage {
    value: uint64
}

struct (0x00000002) Bounce {
    value: coins
}
struct (0x00000003) Destroy {}

type AllowedMessage = Bounce | Destroy

const NO_BOUNCE_PLEASE = 101
const NO_PLEASE = 101

fun onInternalMessage(in: InMessage) {
    val msg = lazy AllowedMessage.fromSlice(in.body);

    var storage = Storage.fromCell(contract.getData());
    storage.value += 1;
    contract.setData(storage.toCell());

    match (msg) {
        Destroy => {
            println("hdestroy");

            val msg = createMessage({
                dest: in.senderAddress,
                value: 0,
                bounce: false,
            });

            msg.send(SEND_MODE_CARRY_ALL_BALANCE + SEND_MODE_DESTROY);
        }

        Bounce => {
            throw msg.value;
        }

        else => {
            assert (in.body.isEmpty()) throw 0xFFFF;
        }
    }
}

fun onBouncedMessage(in: InMessageBounced) {}

get fun get_value(): uint64 {
    val storage = Storage.fromCell(contract.getData());
    return storage.value;
}
