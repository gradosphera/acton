use std::env;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    println!("cargo:rustc-link-search=native={manifest_dir}/assets/libemulator/");
    // println!("cargo:rustc-link-search=native={manifest_dir}/assets/libemulator-opt/");

    pkg_config::Config::new().probe("openssl").unwrap();
    pkg_config::Config::new().probe("libsodium").unwrap();
    pkg_config::Config::new().probe("zlib").unwrap();

    println!("cargo:rustc-link-lib=static=emulator_static");
    println!("cargo:rustc-link-lib=static=smc-envelope");
    println!("cargo:rustc-link-lib=static=tdutils");
    println!("cargo:rustc-link-lib=static=ton_crypto");
    println!("cargo:rustc-link-lib=static=ton_crypto_core");
    println!("cargo:rustc-link-lib=static=ton_block");
    println!("cargo:rustc-link-lib=static=src_parser");
    // Release
    // println!("cargo:rustc-link-lib=static=emulator_static-opt");
    // println!("cargo:rustc-link-lib=static=smc-envelope-opt");
    // println!("cargo:rustc-link-lib=static=tdutils-opt");
    // println!("cargo:rustc-link-lib=static=ton_crypto-opt");
    // println!("cargo:rustc-link-lib=static=ton_crypto_core-opt");
    // println!("cargo:rustc-link-lib=static=ton_block-opt");
    // println!("cargo:rustc-link-lib=static=src_parser-opt");

    println!("cargo:rustc-link-lib=static=absl_hash");
    println!("cargo:rustc-link-lib=static=absl_raw_hash_set");
    println!("cargo:rustc-link-lib=static=absl_hashtablez_sampler");
    println!("cargo:rustc-link-lib=static=absl_low_level_hash");
    println!("cargo:rustc-link-lib=static=absl_base");
    println!("cargo:rustc-link-lib=static=absl_throw_delegate");
    println!("cargo:rustc-link-lib=static=crc32c");
    println!("cargo:rustc-link-lib=static=blst");

    println!("cargo:rustc-link-lib=dylib=ssl");

    println!("cargo:rustc-link-lib=dylib=c++");
    println!("cargo:rustc-link-lib=dylib=c++abi");
}
