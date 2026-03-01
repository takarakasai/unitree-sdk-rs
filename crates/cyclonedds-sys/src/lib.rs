//! Low-level FFI bindings to Eclipse Cyclone DDS (`libddsc`).
//!
//! The bindings are generated at build time by bindgen from `wrapper.h`
//! (`#include <dds/dds.h>`) and restricted to the `dds_*` / `DDS_*` surface.
//! See `build.rs` for header/library resolution.
//!
//! Everything here is `unsafe` FFI; safe wrappers live in the `unitree-dds`
//! crate.
#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
