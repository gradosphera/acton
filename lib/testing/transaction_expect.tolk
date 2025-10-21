import "expect"

type TransactionList = tuple

struct SearchParams {
    to: address
    from: address? = null
    exit_code: int32? = null
    deploy: bool? = null
}

fun Expectation<TransactionList>.toHaveSuccesfulDeploy(self, params: SearchParams) {
    params.exit_code = 0;
    params.deploy = true;

    val tx = findTransactionByParams(self.value, params.__toTuple());
    if (tx == null) {
        failToFindTransactionByParams(
            self.location,
            "expect(actual).toHaveSuccesfulDeploy(expected)",
            self.value,
            params.__toTuple()
        );
        throw 567;
    }
}

fun Expectation<TransactionList>.toHaveSuccesfulTx(self, params: SearchParams, exit_code: int = 0) {
    params.exit_code = exit_code;

    val tx = findTransactionByParams(self.value, params.__toTuple());
    if (tx == null) {
        failToFindTransactionByParams(
            self.location,
            "expect(actual).toHaveSuccesfulTx(expected)",
            self.value,
            params.__toTuple()
        );
        throw 567;
    }
}

fun Expectation<TransactionList>.toHaveFailedTx(self, params: SearchParams) {
    if (params.exit_code == null) {
        Assert.fail("expect(actual).toHaveFailedTx(params) must have not null exit_code parameter in params");
    }
    if (params.exit_code == 0) {
        Assert.fail("expect(actual).toHaveFailedTx(params) must have non zero exit_code parameter in params");
    }

    val tx = findTransactionByParams(self.value, params.__toTuple());
    if (tx == null) {
        failToFindTransactionByParams(
            self.location,
            "expect(actual).toHaveFailedTx(expected)",
            self.value,
            params.__toTuple()
        );
        throw 567;
    }
}

// fun Expectation<TransactionList>.findMessage<Message>(self, criteria: SearchParams): Transaction {
//     // TODO
// }
//
// fun Expectation<TransactionList>.findExternalMessage<Message>(self, criteria: SearchParams): Transaction {
//     // TODO
// }

fun findTransactionByParams(txs: tuple, params: tuple): tuple?
    asm "10 EXTCALL"

fun failToFindTransactionByParams(location: slice, message: slice, txs: tuple, params: tuple): tuple?
    asm "103 EXTCALL"
