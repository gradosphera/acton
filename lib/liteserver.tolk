import "ffi/ffi"

/// Sends a raw query to the LiteServer.
/// The input `data` cell should contain the serialized TL request (without WrappedRequest prefix).
/// Returns a cell with the raw `Answer.answer` bytes from the LiteServer response.
fun liteserverQuery(data: cell): cell {
    return ffi.liteserver_query(data);
}

fun ffi.liteserver_query(data: cell): cell
    asm "31 EXTCALL"
