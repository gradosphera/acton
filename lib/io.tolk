fun println<T>(str: T): void asm "1 EXTCALL"
fun eprintln(str: slice): void asm "2 EXTCALL"
