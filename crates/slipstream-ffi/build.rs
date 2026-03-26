#[path = "build/android.rs"]
mod android;
#[path = "build/cc.rs"]
mod cc;
#[path = "build/openssl.rs"]
mod openssl;
#[path = "build/picoquic.rs"]
mod picoquic;
#[path = "build/util.rs"]
mod util;

use android::maybe_link_android_builtins;
use cc::compile_cc_lib;
use openssl::resolve_openssl_paths;
use picoquic::{
    build_picoquic, locate_picoquic_include_dir, locate_picoquic_lib_dir,
    locate_picotls_include_dir, resolve_picoquic_libs,
};
use std::env;
use std::path::{Path, PathBuf};
use util::env_flag;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=PICOQUIC_DIR");
    println!("cargo:rerun-if-env-changed=PICOQUIC_BUILD_DIR");
    println!("cargo:rerun-if-env-changed=PICOQUIC_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=PICOQUIC_LIB_DIR");
    println!("cargo:rerun-if-env-changed=PICOQUIC_AUTO_BUILD");
    println!("cargo:rerun-if-env-changed=PICOTLS_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=OPENSSL_ROOT_DIR");
    println!("cargo:rerun-if-env-changed=OPENSSL_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=OPENSSL_CRYPTO_LIBRARY");
    println!("cargo:rerun-if-env-changed=OPENSSL_SSL_LIBRARY");
    println!("cargo:rerun-if-env-changed=OPENSSL_USE_STATIC_LIBS");
    println!("cargo:rerun-if-env-changed=OPENSSL_NO_VENDOR");
    println!("cargo:rerun-if-env-changed=DEP_OPENSSL_ROOT");
    println!("cargo:rerun-if-env-changed=DEP_OPENSSL_INCLUDE");
    println!("cargo:rerun-if-env-changed=RUST_ANDROID_GRADLE_CC");
    println!("cargo:rerun-if-env-changed=RUST_ANDROID_GRADLE_AR");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
    println!("cargo:rerun-if-env-changed=ANDROID_ABI");
    println!("cargo:rerun-if-env-changed=ANDROID_PLATFORM");
    println!("cargo:rerun-if-env-changed=CC");
    println!("cargo:rerun-if-env-changed=AR");

    let mut openssl_ssl_lib = None;
    let mut openssl_crypto_lib = None;
    let allow_openssl_env_overrides =
        !cfg!(feature = "openssl-vendored") || env::var_os("OPENSSL_NO_VENDOR").is_some();
    if allow_openssl_env_overrides {
        let openssl_root = env::var_os("OPENSSL_ROOT_DIR");
        let openssl_include = env::var_os("OPENSSL_INCLUDE_DIR");
        let openssl_ssl_lib_env = env::var_os("OPENSSL_SSL_LIBRARY");
        let openssl_crypto_lib_env = env::var_os("OPENSSL_CRYPTO_LIBRARY");

        let has_root = openssl_root.is_some();
        let has_include = openssl_include.is_some();
        let has_ssl = openssl_ssl_lib_env.is_some();
        let has_crypto = openssl_crypto_lib_env.is_some();

        if has_ssl ^ has_crypto {
            return Err(
                "OPENSSL_SSL_LIBRARY and OPENSSL_CRYPTO_LIBRARY must be set together.".into(),
            );
        }

        let has_explicit_libs = has_ssl && has_crypto;
        if has_include && !has_root && !has_explicit_libs {
            return Err(
                "OPENSSL_INCLUDE_DIR without OPENSSL_ROOT_DIR is unsupported; set OPENSSL_ROOT_DIR or both OPENSSL_SSL_LIBRARY and OPENSSL_CRYPTO_LIBRARY (with OPENSSL_INCLUDE_DIR)."
                    .into(),
            );
        }

        if has_explicit_libs && !has_root && !has_include {
            return Err(
                "OPENSSL_SSL_LIBRARY/OPENSSL_CRYPTO_LIBRARY require OPENSSL_INCLUDE_DIR when OPENSSL_ROOT_DIR is not set."
                    .into(),
            );
        }

        openssl_ssl_lib = openssl_ssl_lib_env.map(PathBuf::from);
        openssl_crypto_lib = openssl_crypto_lib_env.map(PathBuf::from);
    }

    let openssl_paths = resolve_openssl_paths();
    let target = env::var("TARGET").unwrap_or_default();
    let auto_build = env_flag("PICOQUIC_AUTO_BUILD", true);
    let explicit_picoquic_include = env::var_os("PICOQUIC_INCLUDE_DIR").is_some();
    let explicit_picoquic_lib = env::var_os("PICOQUIC_LIB_DIR").is_some();
    let explicit_picoquic_include_lib = explicit_picoquic_include || explicit_picoquic_lib;
    let mut picoquic_include_dir = locate_picoquic_include_dir();
    let mut picoquic_lib_dir = locate_picoquic_lib_dir();
    let mut picotls_include_dir = locate_picotls_include_dir();

    if auto_build
        && !explicit_picoquic_include_lib
        && (picoquic_include_dir.is_none() || picoquic_lib_dir.is_none())
    {
        build_picoquic(&openssl_paths, &target)?;
        picoquic_include_dir = locate_picoquic_include_dir();
        picoquic_lib_dir = locate_picoquic_lib_dir();
        picotls_include_dir = locate_picotls_include_dir();
    }

    if explicit_picoquic_include_lib {
        if picoquic_include_dir.is_none() {
            return Err(
                "Explicit PICOQUIC_INCLUDE_DIR/PICOQUIC_LIB_DIR set; missing headers. Set PICOQUIC_INCLUDE_DIR to match your libs."
                .into(),
            );
        }
        if picoquic_lib_dir.is_none() {
            return Err(
                "Explicit PICOQUIC_INCLUDE_DIR/PICOQUIC_LIB_DIR set; missing libs. Set PICOQUIC_LIB_DIR to match your headers."
                .into(),
            );
        }
    }

    let picoquic_include_dir = picoquic_include_dir.ok_or(
        "Missing picoquic headers; set PICOQUIC_DIR or PICOQUIC_INCLUDE_DIR (default: vendor/picoquic).",
    )?;
    let picoquic_lib_dir = picoquic_lib_dir.ok_or(
        "Missing picoquic build artifacts; run ./scripts/build_picoquic.sh or set PICOQUIC_BUILD_DIR/PICOQUIC_LIB_DIR.",
    )?;
    let picotls_include_dir = picotls_include_dir.ok_or(
        "Missing picotls headers; set PICOTLS_INCLUDE_DIR or build picoquic with PICOQUIC_FETCH_PTLS=ON.",
    )?;

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let cc_dir = manifest_dir.join("cc");
    let cc_src = cc_dir.join("slipstream_server_cc.c");
    let mixed_cc_src = cc_dir.join("slipstream_mixed_cc.c");
    let poll_src = cc_dir.join("slipstream_poll.c");
    let stateless_packet_src = cc_dir.join("slipstream_stateless_packet.c");
    let test_helpers_src = cc_dir.join("slipstream_test_helpers.c");
    let picotls_layout_src = cc_dir.join("picotls_layout.c");
    println!("cargo:rerun-if-changed={}", cc_src.display());
    println!("cargo:rerun-if-changed={}", mixed_cc_src.display());
    println!("cargo:rerun-if-changed={}", poll_src.display());
    println!("cargo:rerun-if-changed={}", stateless_packet_src.display());
    println!("cargo:rerun-if-changed={}", test_helpers_src.display());
    println!("cargo:rerun-if-changed={}", picotls_layout_src.display());
    let picoquic_internal = picoquic_include_dir.join("picoquic_internal.h");
    if picoquic_internal.exists() {
        println!("cargo:rerun-if-changed={}", picoquic_internal.display());
    }

    // Compile C helpers using the cc crate (auto-detects GCC/Clang/MSVC).
    compile_cc_lib(
        "slipstream_helpers",
        &[
            cc_src,
            mixed_cc_src,
            poll_src,
            stateless_packet_src,
            test_helpers_src,
        ],
        &[&picoquic_include_dir],
    );
    compile_cc_lib(
        "picotls_layout",
        &[picotls_layout_src],
        &[&picoquic_include_dir, &picotls_include_dir],
    );

    let picoquic_libs = resolve_picoquic_libs(&picoquic_lib_dir).ok_or(
        "Missing picoquic build artifacts; run ./scripts/build_picoquic.sh or set PICOQUIC_BUILD_DIR/PICOQUIC_LIB_DIR.",
    )?;
    for dir in picoquic_libs.search_dirs {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    for lib in picoquic_libs.libs {
        println!("cargo:rustc-link-lib=static={}", lib);
    }

    if !cfg!(feature = "openssl-vendored") {
        let mut openssl_search_dirs = Vec::new();
        if let Some(lib) = &openssl_ssl_lib {
            add_parent_dir(&mut openssl_search_dirs, lib);
        }
        if let Some(lib) = &openssl_crypto_lib {
            add_parent_dir(&mut openssl_search_dirs, lib);
        }
        if openssl_search_dirs.is_empty() {
            if let Some(root) = &openssl_paths.root {
                push_unique_dir(&mut openssl_search_dirs, root.join("lib"));
                push_unique_dir(&mut openssl_search_dirs, root.join("lib64"));
            }
        }
        for dir in openssl_search_dirs {
            println!("cargo:rustc-link-search=native={}", dir.display());
        }
        if cfg!(feature = "openssl-static") {
            println!("cargo:rustc-link-lib=static=ssl");
            println!("cargo:rustc-link-lib=static=crypto");
        } else {
            println!("cargo:rustc-link-lib=dylib=ssl");
            println!("cargo:rustc-link-lib=dylib=crypto");
        }
    }

    if target.contains("windows") {
        println!("cargo:rustc-link-lib=dylib=ws2_32");
        println!("cargo:rustc-link-lib=dylib=bcrypt");
    } else if target.contains("android") {
        maybe_link_android_builtins(&target, &env::var("CC").unwrap_or_default());
    } else {
        println!("cargo:rustc-link-lib=dylib=pthread");
    }

    Ok(())
}

fn push_unique_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dirs.iter().any(|existing| existing == &dir) {
        dirs.push(dir);
    }
}

fn add_parent_dir(dirs: &mut Vec<PathBuf>, path: &Path) {
    if let Some(parent) = path.parent() {
        push_unique_dir(dirs, parent.to_path_buf());
    }
}
