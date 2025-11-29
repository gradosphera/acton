struct fs

fun fs.readFile(path: slice): slice | null asm "3 EXTCALL"
