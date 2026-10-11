// The in-memory filesystem (../driver/memfs.cc) is C++ and sits under the C
// library in both builds: natively and for the sandbox. It is compiled here
// so that whoever links this crate has it.
fn main() {
    let guest = std::env::var("TARGET").map(|t| t.contains("waterbox")).unwrap_or(false);
    let mut build = cc::Build::new();
    build.cpp(true).std("c++20").file("../driver/memfs.cc").file("../driver/memfs-c.cc").include("../driver");
    if guest {
        build.define("CHIMERA_GUEST", None);
    }
    build.compile("chimera_memfs");
    println!("cargo:rerun-if-changed=../driver/memfs.cc");
    println!("cargo:rerun-if-changed=../driver/memfs-c.cc");
    println!("cargo:rerun-if-changed=../driver/memfs.h");
}
