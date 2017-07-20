// cortex-m-quickstart build.rs
// Copyright © 2017 Jorge Aparicio
// Licensed under the Apache license v2.0, or the MIT license.
// See https://github.com/japaric/cortex-m-quickstart for details.

extern crate gcc;

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    //gcc::compile_library("libold_c.a", &["src/old_c/main.c"]);
    gcc::Config::new()
        .file("src/old_c/ST7735.c")
        // .define("FOO", Some("bar"))
        // .include("/opt/stm32f3/STM32F3-Discovery_FW_V1.1.0/Libraries/CMSIS/Device/ST/STM32F30x/Include/")
        // .include("/opt/stm32f3/STM32F3-Discovery_FW_V1.1.0/Libraries/CMSIS/Include/")
        // gcc-crate defaults to PIC, which results in a .got (global offset
        // table) section that doesn't get relocated properly. Turn off PIC to fix.
        // See https://github.com/japaric/cortex-m-rt/issues/22
        .pic(false)
        .compile("libold_c.a");

    // Put the linker script somewhere the linker can find it
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=memory.x");
}
