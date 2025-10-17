struct fs

fun fs.read_file(path: slice): slice | null asm "3 EXTCALL"
