import "../testing/transaction_expect"

fun build(name: slice, path: slice): cell asm "6 EXTCALL"

fun sendMessage(msg: cell, mode: int): TransactionList? asm "7 EXTCALL"
fun sendMessageFrom(msg: cell, from: cell, mode: int): TransactionList? asm "9 EXTCALL"

fun runGetMethod(addr: cell, code: cell, id: int, return_type: slice, args: tuple): tuple asm "8 EXTCALL"
