/*
Copyright 2021 Adobe
All Rights Reserved.

NOTICE: Adobe permits you to use, modify, and distribute this file in
accordance with the terms of the Adobe license agreement accompanying
it.
*/
fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    if target_os == "macos" {
        // on Mac, rust requires thin libraries, so we build one for each architecture
        println!("cargo:rustc-link-lib=static=ngl.{}", &target_arch);
        // provide the library location
        println!("cargo:rustc-link-search=native=rsrc/libraries/macos");
        // ngl requires some system-provided libraries
        println!("cargo:rustc-link-lib=dylib=c++");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=IOKit");
    } else if target_os == "windows" {
        // on Windows, rust only does x86_64, so there's only one library
        println!("cargo:rustc-link-lib=static=ngl");
        // provide the library location
        println!("cargo:rustc-link-search=native=windows");
        // ngl requires some system-provided libraries
        println!("cargo:rustc-link-lib=dylib=libcpmt");
    }
}
