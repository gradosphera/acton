/// HTTP fetch module backed by host-side FFI.
///
/// The module is intended for scripts/tests and not for on-chain execution.
///
/// Header format:
/// - `headers` is an array of `[name, value]` pairs
/// - Example: `[["accept", "application/json"], ["x-api-key", "secret"]] as FetchHeaders`
///
/// On network/request/argument errors, all methods return `null`.

import "ffi/ffi"
import "json"
import "@stdlib/strings"

/// Namespace for network HTTP helpers.
struct fetch

/// HTTP header as `[name, value]`.
type FetchHeader = [string, string]

/// HTTP headers as an array of [FetchHeader].
type FetchHeaders = array<[string, string]>

/// Raw HTTP response from [fetch.request].
///
/// `headers` is an array of `[name, value]` pairs.
struct FetchResponse {
    status: int
    ok: bool
    body: string
    headers: FetchHeaders
}

/// Performs an HTTP request.
fun fetch.request(
    method: string,
    url: string,
    body: string = "",
    headers: FetchHeaders = [],
): FetchResponse? {
    val response = ffi.fetchRequest(method, url, body, headers);
    if (response == null) {
        return null;
    }
    return FetchResponse.fromTuple(response);
}

/// Performs an HTTP GET request.
fun fetch.get(url: string, headers: FetchHeaders = []): FetchResponse? {
    return fetch.request("GET", url, "", headers);
}

/// Performs an HTTP POST request.
fun fetch.post(url: string, body: string, headers: FetchHeaders = []): FetchResponse? {
    return fetch.request("POST", url, body, headers);
}

/// Returns the first response header value matching `name`.
///
/// Header lookup is case-sensitive.
fun FetchResponse.header(self, name: string): string? {
    var i = 0;
    while (i < self.headers.size()) {
        val [key, value] = self.headers.get(i) as FetchHeader;
        if (key.equalTo(name)) {
            return value;
        }
        i += 1;
    }
    return null;
}

/// Applies a jq-like JSON query to response body.
fun FetchResponse.json<T = string>(self, query: string): T? {
    return json.query<T>(self.body, query);
}

/// Applies a jq-like JSON query to response body and returns all matches.
fun FetchResponse.jsonAll(self, query: string): tuple {
    return json.queryAll(self.body, query);
}

fun ffi.fetchRequest(method: string, url: string, body: string, headers: FetchHeaders): tuple?
    asm "56 EXTCALL"
