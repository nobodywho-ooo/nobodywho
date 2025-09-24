fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target.contains("android") {
        println!("cargo:rustc-link-lib=c++_shared");
    }
}
