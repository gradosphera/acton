use std::env;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    println!("cargo:rustc-link-search=native={manifest_dir}/../../objs/");
    println!("cargo:rustc-link-lib=static=tolk");
    println!("cargo:rerun-if-changed={manifest_dir}/../../objs/libtolk.a");
}
