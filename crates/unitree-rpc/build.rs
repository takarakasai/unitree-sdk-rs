//! Build script for `unitree-rpc`.
//!
//! Compiles the committed idlc-generated topic descriptors for the Unitree RPC
//! envelope types (`unitree_api::msg::dds_::Request_` / `Response_`) into a
//! static lib, using the Cyclone DDS headers exported by `cyclonedds-sys`
//! (`DEP_DDSC_INCLUDE_DIR`, available because that crate has `links = "ddsc"`).
//!
//! `csrc/rpc.{idl,c,h}` are committed (regenerate with idlc only if the RPC IDL
//! changes), so a normal build needs just a C compiler — never idlc:
//!
//!   idlc -l c csrc/rpc.idl   # -> csrc/rpc.c, csrc/rpc.h
//!
//! The `Request_` / `Response_` descriptors are variable-length (they carry a
//! `string` and a `sequence<octet>`), which is exactly why they live here and
//! not in `unitree-dds`: that crate's POD-only data path cannot exchange them.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let csrc = manifest_dir.join("csrc");
    let include_dir = std::env::var("DEP_DDSC_INCLUDE_DIR")
        .expect("DEP_DDSC_INCLUDE_DIR not set by cyclonedds-sys");

    let c_file = csrc.join("rpc.c");
    println!("cargo:rerun-if-changed={}", c_file.display());
    println!("cargo:rerun-if-changed={}", csrc.join("rpc.h").display());

    cc::Build::new()
        .include(&csrc)
        .include(&include_dir)
        .warnings(false)
        .file(&c_file)
        .compile("unitree_rpc_descriptors");
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing env var {k}"))
}
