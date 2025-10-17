struct Storage {
    counter: int32
}

fun onInternalMessage(in: InMessage) {
    var storage = Storage.fromCell(contract.getData());
    storage.counter += 5;

    contract.setData(storage.toCell())
}

get fun some_data(): int {
    return 120
}

get fun some_other_data(): (int, int) {
    var storage = Storage.fromCell(contract.getData());
    return (42, storage.counter)
}
