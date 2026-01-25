import "lib/emulation/config"
import "lib/emulation/network"
import "crates/tolkc/assets/tolk-stdlib/gas-payments"

fun onInternalMessage(in: InMessage) {
    val config = net.getConfig();

    val version = config.getGlobalVersion();
    version.version = 13;
    config.setGlobalVersion(version);
    val res = net.setConfig(config);

    val gasPrice = config.getGasPrices(BASECHAIN);
    gasPrice.other.blockGasLimit = 10;


    val precompiled = config.getStoragePrices();
    val cont = precompiled
    precompiled.list.addIfNotExists(cont.hash(), 123);

    // net.runGetMethod();

    //
    val fees = calculateGasFee(0, 1000);

}
