//! Build script for `unitree-go2`.
//!
//! Compiles the committed idlc-generated topic descriptor for
//! `std_msgs::msg::dds_::String_` (the `rt/utlidar/switch` topic type) into a
//! static lib, using the Cyclone DDS headers exported by `cyclonedds-sys`
//! (`DEP_DDSC_INCLUDE_DIR`, available because that crate has `links = "ddsc"`).
//!
//! `csrc/std_msgs.{idl,c,h}` are committed (regenerate with idlc only if the
//! IDL changes), so a normal build needs just a C compiler — never idlc:
//!
//!   idlc -l c csrc/std_msgs.idl   # -> csrc/std_msgs.c, csrc/std_msgs.h
//!
//! `String_` is variable-length (it carries a `string`), which is why it lives
//! here / in the `utlidar` module rather than in `unitree-dds`: that crate's
//! POD-only data path cannot exchange it.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let csrc = manifest_dir.join("csrc");
    let include_dir = std::env::var("DEP_DDSC_INCLUDE_DIR")
        .expect("DEP_DDSC_INCLUDE_DIR not set by cyclonedds-sys");

    let c_file = csrc.join("std_msgs.c");
    println!("cargo:rerun-if-changed={}", c_file.display());
    println!("cargo:rerun-if-changed={}", csrc.join("std_msgs.h").display());

    cc::Build::new()
        .include(&csrc)
        .include(&include_dir)
        .warnings(false)
        .file(&c_file)
        .compile("unitree_go2_descriptors");
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing env var {k}"))
}
