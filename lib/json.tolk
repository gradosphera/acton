/// JSON helper module built on host-side FFI.
///
/// Query evaluation is implemented by host-side JSONPath library.
/// For compatibility, jq-like shortcuts are normalized automatically:
/// - `.` for root
/// - `.field` for object fields
/// - `[index]` for array index
/// - `[]` for iterating arrays (normalized to `[*]`)
/// - bracket-quoted keys like `.["field.with.dots"]`
///
/// You can also pass a direct JSONPath expression starting with `$`.
///
/// Returns:
/// - `null` on invalid JSON, invalid query, or when no value is found
/// - strings as raw text
/// - numbers/bools/objects/arrays as JSON text
///
/// Examples:
/// ```tolk
/// val data = "{\"items\":[{\"id\":1},{\"id\":2}]}";
/// val firstId = json.query(data, ".items[0].id"); // "1"
/// val allIds = json.queryAll(data, ".items[].id"); // ["1", "2"] as tuple
/// ```

import "ffi/ffi"

/// Namespace for JSON query helpers.
struct json

/// Selects the first value matched by a jq-like query.
fun json.query<T = string>(jsonText: string, query: string): T? {
    return ffi.jsonQuery<T>(jsonText, query);
}

/// Selects all values matched by a jq-like query.
///
/// Returns an empty tuple when query is valid but produced no matches.
/// Returns an empty tuple on parse/query failure as well.
fun json.queryAll(jsonText: string, query: string): tuple {
    val value = ffi.jsonQueryAll(jsonText, query);
    if (value == null) {
        return [];
    }
    return value;
}

fun ffi.jsonQuery<T>(jsonText: string, query: string): T? asm "57 EXTCALL"
fun ffi.jsonQueryAll(jsonText: string, query: string): tuple? asm "58 EXTCALL"
