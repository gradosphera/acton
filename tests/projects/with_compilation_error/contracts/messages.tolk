struct (0x00000001) Msg
struct (0x00000002) Msg2

struct Storage

fun Storage.load() {
    return Storage.fromCell(contract.getData());
}

fun Storage.save(self) {
    contract.setData(self.toCell());
}
