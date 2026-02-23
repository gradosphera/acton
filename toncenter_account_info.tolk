import "@stdlib/reflection"
import "lib/env"
import "lib/fetch"
import "lib/fmt"
import "lib/io"
import "@stdlib/strings"

const DEFAULT_ADDRESS = "EQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAM9c";

fun buildToncenterUrl(network: string, address: string): string {
    if (network.equalTo("mainnet")) {
        return format1("https://toncenter.com/api/v2/getAddressInformation?address={}", address);
    }

    return format1("https://testnet.toncenter.com/api/v2/getAddressInformation?address={}", address);
}

fun buildHeaders(apiKey: string): array<[string, string]> {
    if (apiKey.equalTo("")) {
        return [
            ["accept", "application/json"],
        ];
    }

    return [
        ["accept", "application/json"],
        ["X-API-Key", apiKey]
    ];
}

fun asStringOr(value: string?, fallback: string): string {
    if (value == null) {
        return fallback;
    }
    return value;
}

fun main() {
    val network = envOr<string>("TONCENTER_NETWORK", "testnet");
    val address = envOr<string>("TONCENTER_ACCOUNT", DEFAULT_ADDRESS);
    val apiKey = envOr<string>("TONCENTER_API_KEY", "");

    val url = buildToncenterUrl(network, address);
    val response = fetch.get(url, buildHeaders(apiKey));
    println(response);
    if (response == null) {
        eprintln("Request failed: unable to reach Toncenter API");
        return;
    }

    if (!response.ok) {
        eprintln(format2("Toncenter HTTP error {}. Body: {}", response.status, response.body));
        return;
    }

    val apiOk = response.json<string>(".ok");
    println(apiOk);
    if (apiOk == null) {
        eprintln("Toncenter returned `ok = false`");
        eprintln(response.body);
        return;
    }

    println1("Network: {}", network);
    println1("Address: {}", address);
    println1("State: {}", response.json(".result.state"));
    println1(
        "Balance (nanotons): {}",
        response.json(".result.balance")
    );
    println1("Last tx lt: {}", response.json(".result.last_transaction_id.lt"));
    println1(
        "Last tx hash: {}",
        response.json(".result.last_transaction_id.hash")
    );
}
