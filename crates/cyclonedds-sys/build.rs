//! Build script for `cyclonedds-sys`.
//!
//! Resolves the Cyclone DDS headers and `libddsc` shared library, generates
//! FFI bindings with bindgen, and configures linking so both `cargo build` and
//! test execution find the library at runtime.
//!
//! Library/header resolution order:
//! 1. `CYCLONEDDS_HOME` — a Cyclone DDS install with `include/` and `lib/`.
//! 2. `UNITREE_SDK2_ROOT` — uses `<root>/thirdparty/{include,lib/<arch>}`.
//! 3. The vendored copy under `vendor/cyclonedds/` in this repo (default).
//!
//! The vendored `libddsc.so` has SONAME `libddsc.so.0`, but only the linker
//! name `libddsc.so` is shipped. We stage both names into `OUT_DIR` and add an
//! rpath there so test binaries load the library without `LD_LIBRARY_PATH`.

use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf();
    let arch = env("CARGO_CFG_TARGET_ARCH"); // "x86_64" | "aarch64"
    let out_dir = PathBuf::from(env("OUT_DIR"));

    println!("cargo:rerun-if-env-changed=CYCLONEDDS_HOME");
    println!("cargo:rerun-if-env-changed=UNITREE_SDK2_ROOT");
    println!("cargo:rerun-if-changed=wrapper.h");

    ensure_libclang();

    let (include_dir, lib_dir) = resolve_dirs(&workspace_root, &arch);

    let header = include_dir.join("dds/dds.h");
    assert!(
        header.exists(),
        "Cyclone DDS header not found at {} — set CYCLONEDDS_HOME or UNITREE_SDK2_ROOT",
        header.display()
    );

    // --- bindgen ---
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include_dir.display()))
        // Restrict to the DDS C surface to keep bindings small and stable.
        .allowlist_function("dds_.*")
        .allowlist_type("dds_.*")
        .allowlist_type("DDS_.*")
        .allowlist_var("DDS_.*")
        .allowlist_var("dds_.*")
        .default_enum_style(bindgen::EnumVariation::ModuleConsts)
        .derive_default(true)
        .layout_tests(false)
        .generate()
        .expect("bindgen failed to generate Cyclone DDS bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("write bindings.rs");

    // --- stage the shared library with its SONAME so runtime load works ---
    let src_so = lib_dir.join("libddsc.so");
    assert!(
        src_so.exists(),
        "libddsc.so not found at {}",
        src_so.display()
    );
    stage_lib(&src_so, &out_dir);

    // Link against ddsc from OUT_DIR (where both .so and .so.0 now live).
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=dylib=ddsc");
    // rpath so `cargo test`/examples load libddsc without LD_LIBRARY_PATH.
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", out_dir.display());

    // --- compile the committed topic descriptor(s) used by tests ---
    //
    // The C descriptor in `csrc/` is generated from `echo.idl` with idlc and
    // committed so that builds need only a C compiler, not idlc. We compile it
    // into a small static lib that the echo integration test links against.
    // (Descriptor handling proper lives in the unitree-dds backend; this is a
    // minimal hook to exercise the FFI end to end.)
    let csrc = manifest_dir.join("csrc");
    let echo_c = csrc.join("echo.c");
    if echo_c.exists() {
        println!("cargo:rerun-if-changed={}", echo_c.display());
        println!("cargo:rerun-if-changed={}", csrc.join("echo.h").display());
        cc::Build::new()
            .file(&echo_c)
            .include(&csrc)
            .include(&include_dir)
            .warnings(false)
            .compile("cyclonedds_test_descriptors");
    }

    // Re-export the resolved lib dir for dependent crates / debugging.
    println!("cargo:lib_dir={}", lib_dir.display());
    println!("cargo:include_dir={}", include_dir.display());
}

/// Ensure bindgen can locate libclang. If `LIBCLANG_PATH` is unset, probe the
/// common Debian/Ubuntu llvm install dirs (apt installs `libclang-NN.so.NN`,
/// which older clang-sys does not always find on its own).
fn ensure_libclang() {
    println!("cargo:rerun-if-env-changed=LIBCLANG_PATH");
    if env_opt("LIBCLANG_PATH").is_some() {
        return;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    // Versioned llvm dirs (newest first).
    if let Ok(entries) = std::fs::read_dir("/usr/lib") {
        let mut llvm: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("llvm-"))
            })
            .map(|p| p.join("lib"))
            .collect();
        llvm.sort();
        llvm.reverse();
        candidates.extend(llvm);
    }
    // Multiarch dir where `libclang-NN.so.NN` typically lives.
    candidates.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
    candidates.push(PathBuf::from("/usr/lib/aarch64-linux-gnu"));

    for dir in candidates {
        if !dir.is_dir() {
            continue;
        }
        let has_libclang = std::fs::read_dir(&dir)
            .ok()
            .map(|rd| {
                rd.filter_map(Result::ok).any(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.starts_with("libclang") && n.contains(".so"))
                })
            })
            .unwrap_or(false);
        if has_libclang {
            println!("cargo:warning=cyclonedds-sys: using libclang from {}", dir.display());
            std::env::set_var("LIBCLANG_PATH", &dir);
            return;
        }
    }
    // Fall through: let bindgen produce its own descriptive error.
}

/// Resolve (include_dir, lib_dir) from env overrides or the vendored copy.
fn resolve_dirs(workspace_root: &Path, arch: &str) -> (PathBuf, PathBuf) {
    if let Some(home) = env_opt("CYCLONEDDS_HOME") {
        let home = PathBuf::from(home);
        let lib = first_existing(&[home.join("lib").join(arch), home.join("lib")])
            .unwrap_or_else(|| home.join("lib"));
        return (home.join("include"), lib);
    }
    if let Some(root) = env_opt("UNITREE_SDK2_ROOT") {
        let tp = PathBuf::from(root).join("thirdparty");
        return (tp.join("include"), tp.join("lib").join(arch));
    }
    let vendor = workspace_root.join("vendor/cyclonedds");
    (vendor.join("include"), vendor.join("lib").join(arch))
}

/// Copy `libddsc.so` into `out_dir` as both `libddsc.so` (linker name) and
/// `libddsc.so.0` (SONAME, needed by the runtime loader).
fn stage_lib(src_so: &Path, out_dir: &Path) {
    for name in ["libddsc.so", "libddsc.so.0"] {
        let dst = out_dir.join(name);
        // Always refresh to track upstream changes.
        let _ = std::fs::remove_file(&dst);
        std::fs::copy(src_so, &dst)
            .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src_so.display(), dst.display()));
    }
    println!("cargo:rerun-if-changed={}", src_so.display());
}

fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.exists()).cloned()
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("missing env var {k}"))
}

fn env_opt(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|s| !s.is_empty())
}
