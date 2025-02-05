fn main() {
    // Force rebuild if any of these environment variables change
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    println!("cargo:rerun-if-changed=build.rs");
}
