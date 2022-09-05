/*
Copyright 2022 Daniel Brotsky. All rights reserved.

All of the copyrighted work in this repository is licensed under the
GNU Affero General Public License, reproduced in the LICENSE-AGPL file.

Attribution:

Some source files in this repository are derived from files in two Adobe Open
Source projects: the Adobe License Decoder repository found at this URL:
    https://github.com/adobe/adobe-license-decoder.rs
and the FRL Online Proxy repository found at this URL:
    https://github.com/adobe/frl-online-proxy

The files in those original works are copyright 2022 Adobe and the use of those
materials in this work is permitted by the MIT license under which they were
released.  That license is reproduced here in the LICENSE-MIT file.
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
        println!("cargo:rustc-link-search=native=rsrc/libraries/windows");
        // ngl requires some system-provided libraries
        println!("cargo:rustc-link-lib=dylib=libcpmt");
    }
}
