use crate::openssl::OpenSslPaths;
use crate::util::locate_repo_root;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct PicoquicLibs {
    pub(crate) search_dirs: Vec<PathBuf>,
    pub(crate) libs: Vec<&'static str>,
}

pub(crate) fn build_picoquic(
    openssl_paths: &OpenSslPaths,
    target: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = locate_repo_root().ok_or("Could not locate repository root for picoquic build")?;
    let picoquic_dir = env::var_os("PICOQUIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("vendor").join("picoquic"));
    if !picoquic_dir.exists() {
        return Err("picoquic submodule missing; run git submodule update --init --recursive vendor/picoquic".into());
    }
    let build_dir = env::var_os("PICOQUIC_BUILD_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".picoquic-build"));

    // Use native cmake when:
    // - On Windows (no bash available)
    // - On macOS cross-compiling (need CMAKE_OSX_ARCHITECTURES the bash script doesn't pass)
    // Otherwise prefer the shell script for backward compatibility.
    let is_macos_cross = cfg!(target_os = "macos") && {
        let host_arch = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x86_64"
        };
        !target.contains(host_arch)
    };
    let use_script = !cfg!(target_os = "windows") && !is_macos_cross && {
        let script = root.join("scripts").join("build_picoquic.sh");
        script.exists()
    };

    if use_script {
        let script = root.join("scripts").join("build_picoquic.sh");
        let mut command = Command::new(script);
        command
            .env("PICOQUIC_DIR", &picoquic_dir)
            .env("PICOQUIC_BUILD_DIR", &build_dir)
            .env("PICOQUIC_TARGET", target);
        if target.contains("android") {
            if let Ok(value) = env::var("ANDROID_NDK_HOME") {
                command.env("ANDROID_NDK_HOME", value);
            }
            if let Ok(value) = env::var("ANDROID_ABI") {
                command.env("ANDROID_ABI", value);
            }
            if let Ok(value) = env::var("ANDROID_PLATFORM") {
                command.env("ANDROID_PLATFORM", value);
            }
        }
        if let Some(root) = &openssl_paths.root {
            command.env("OPENSSL_ROOT_DIR", root);
        }
        let prefer_static_openssl = cfg!(feature = "openssl-static");
        let openssl_no_vendor = env::var_os("OPENSSL_NO_VENDOR").is_some();
        let explicit_static = env::var_os("OPENSSL_USE_STATIC_LIBS").is_some();
        if prefer_static_openssl && !openssl_no_vendor && !explicit_static {
            command.env("OPENSSL_USE_STATIC_LIBS", "TRUE");
        }
        if let Some(include) = &openssl_paths.include {
            command.env("OPENSSL_INCLUDE_DIR", include);
        }
        let status = command.status()?;
        if !status.success() {
            return Err(
                "picoquic auto-build failed (run scripts/build_picoquic.sh for details)".into(),
            );
        }
    } else {
        // Native cmake invocation (works on Windows, macOS cross-compile, and anywhere without bash).
        let mut cmake_args: Vec<String> = vec![
            "-DCMAKE_BUILD_TYPE=Release".to_string(),
            "-DPICOQUIC_FETCH_PTLS=ON".to_string(),
            "-DCMAKE_POSITION_INDEPENDENT_CODE=ON".to_string(),
            "-DCMAKE_POLICY_VERSION_MINIMUM=3.5".to_string(),
        ];

        // Windows ARM64 cross-compilation (MSVC Visual Studio generator uses -A flag).
        if target.contains("aarch64") && target.contains("windows") {
            cmake_args.push("-A".to_string());
            cmake_args.push("ARM64".to_string());
        }

        // macOS cross-compilation architecture flag.
        if target.contains("apple") {
            if target.contains("x86_64") {
                cmake_args.push("-DCMAKE_OSX_ARCHITECTURES=x86_64".to_string());
            } else if target.contains("aarch64") {
                cmake_args.push("-DCMAKE_OSX_ARCHITECTURES=arm64".to_string());
            }
        }

        if let Some(root) = &openssl_paths.root {
            cmake_args.push(format!("-DOPENSSL_ROOT_DIR={}", root.display()));
        }
        if let Some(include) = &openssl_paths.include {
            cmake_args.push(format!("-DOPENSSL_INCLUDE_DIR={}", include.display()));
        }
        let prefer_static_openssl = cfg!(feature = "openssl-static");
        let openssl_no_vendor = env::var_os("OPENSSL_NO_VENDOR").is_some();
        let explicit_static = env::var_os("OPENSSL_USE_STATIC_LIBS").is_some();
        if prefer_static_openssl && !openssl_no_vendor && !explicit_static {
            cmake_args.push("-DOPENSSL_USE_STATIC_LIBS=TRUE".to_string());
        }

        let configure = Command::new("cmake")
            .arg("-S")
            .arg(&picoquic_dir)
            .arg("-B")
            .arg(&build_dir)
            .args(&cmake_args)
            .status()?;
        if !configure.success() {
            return Err("picoquic cmake configure failed".into());
        }

        // Workaround: picotls on Windows includes "wincompat.h" from picotls.h,
        // but the win/ directory isn't added to include paths by its cmake.
        // Copy wincompat.h to the include/ directory so the compiler can find it.
        if target.contains("windows") {
            let wincompat_src = build_dir
                .join("_deps")
                .join("picotls-src")
                .join("win")
                .join("wincompat.h");
            let wincompat_dst = build_dir
                .join("_deps")
                .join("picotls-src")
                .join("include")
                .join("wincompat.h");
            if wincompat_src.exists() && !wincompat_dst.exists() {
                std::fs::copy(&wincompat_src, &wincompat_dst).map_err(|e| {
                    format!("Failed to copy wincompat.h: {}", e)
                })?;
            }
        }
        let build = Command::new("cmake")
            .arg("--build")
            .arg(&build_dir)
            .arg("--config")
            .arg("Release")
            .status()?;
        if !build.success() {
            return Err("picoquic cmake build failed".into());
        }
    }
    Ok(())
}

pub(crate) fn locate_picoquic_include_dir() -> Option<PathBuf> {
    if let Ok(dir) = env::var("PICOQUIC_INCLUDE_DIR") {
        let candidate = PathBuf::from(dir);
        if has_picoquic_internal_header(&candidate) {
            return Some(candidate);
        }
    }

    if let Ok(dir) = env::var("PICOQUIC_DIR") {
        let candidate = PathBuf::from(&dir);
        if has_picoquic_internal_header(&candidate) {
            return Some(candidate);
        }
        let candidate = Path::new(&dir).join("picoquic");
        if has_picoquic_internal_header(&candidate) {
            return Some(candidate);
        }
    }

    if let Some(root) = locate_repo_root() {
        let candidate = root.join("vendor").join("picoquic").join("picoquic");
        if has_picoquic_internal_header(&candidate) {
            return Some(candidate);
        }
    }

    None
}

pub(crate) fn locate_picoquic_lib_dir() -> Option<PathBuf> {
    if let Ok(dir) = env::var("PICOQUIC_LIB_DIR") {
        let candidate = PathBuf::from(dir);
        if has_picoquic_libs(&candidate) {
            return Some(candidate);
        }
    }

    if let Ok(dir) = env::var("PICOQUIC_BUILD_DIR") {
        let candidate = PathBuf::from(&dir);
        if has_picoquic_libs(&candidate) {
            return Some(candidate);
        }
        let candidate = Path::new(&dir).join("picoquic");
        if has_picoquic_libs(&candidate) {
            return Some(candidate);
        }
    }

    if let Some(root) = locate_repo_root() {
        let candidate = root.join(".picoquic-build");
        if has_picoquic_libs(&candidate) {
            return Some(candidate);
        }
        let candidate = root.join(".picoquic-build").join("picoquic");
        if has_picoquic_libs(&candidate) {
            return Some(candidate);
        }
    }

    None
}

pub(crate) fn locate_picotls_include_dir() -> Option<PathBuf> {
    if let Ok(dir) = env::var("PICOTLS_INCLUDE_DIR") {
        let candidate = PathBuf::from(dir);
        if has_picotls_header(&candidate) {
            return Some(candidate);
        }
    }

    if let Ok(dir) = env::var("PICOQUIC_BUILD_DIR") {
        let candidate = Path::new(&dir)
            .join("_deps")
            .join("picotls-src")
            .join("include");
        if has_picotls_header(&candidate) {
            return Some(candidate);
        }
    }

    if let Ok(dir) = env::var("PICOQUIC_LIB_DIR") {
        let candidate = Path::new(&dir)
            .join("_deps")
            .join("picotls-src")
            .join("include");
        if has_picotls_header(&candidate) {
            return Some(candidate);
        }
        if let Some(parent) = Path::new(&dir).parent() {
            let candidate = parent.join("_deps").join("picotls-src").join("include");
            if has_picotls_header(&candidate) {
                return Some(candidate);
            }
        }
    }

    if let Some(root) = locate_repo_root() {
        let candidate = root
            .join(".picoquic-build")
            .join("_deps")
            .join("picotls-src")
            .join("include");
        if has_picotls_header(&candidate) {
            return Some(candidate);
        }
        let candidate = root
            .join("vendor")
            .join("picoquic")
            .join("picotls")
            .join("include");
        if has_picotls_header(&candidate) {
            return Some(candidate);
        }
    }

    None
}

pub(crate) fn resolve_picoquic_libs(dir: &Path) -> Option<PicoquicLibs> {
    if let Some(libs) = resolve_picoquic_libs_single_dir(dir) {
        return Some(PicoquicLibs {
            search_dirs: vec![dir.to_path_buf()],
            libs,
        });
    }

    let mut picotls_dirs = vec![dir.join("_deps").join("picotls-build")];
    if let Some(parent) = dir.parent() {
        picotls_dirs.push(parent.join("_deps").join("picotls-build"));
    }
    for picotls_dir in picotls_dirs {
        if let Some(libs) = resolve_picoquic_libs_split(dir, &picotls_dir) {
            let mut search_dirs = vec![dir.to_path_buf()];
            if picotls_dir != dir && !search_dirs.contains(&picotls_dir) {
                search_dirs.push(picotls_dir);
            }
            return Some(PicoquicLibs { search_dirs, libs });
        }
    }

    if let Some(parent) = dir.parent() {
        if let Some(libs) = resolve_picoquic_libs_split(parent, dir) {
            return Some(PicoquicLibs {
                search_dirs: vec![parent.to_path_buf(), dir.to_path_buf()],
                libs,
            });
        }
        if let Some(grandparent) = parent.parent() {
            if let Some(libs) = resolve_picoquic_libs_split(grandparent, dir) {
                return Some(PicoquicLibs {
                    search_dirs: vec![grandparent.to_path_buf(), dir.to_path_buf()],
                    libs,
                });
            }
        }
    }

    None
}

fn has_picoquic_internal_header(dir: &Path) -> bool {
    dir.join("picoquic_internal.h").exists()
}

fn has_picotls_header(dir: &Path) -> bool {
    dir.join("picotls.h").exists()
}

fn has_picoquic_libs(dir: &Path) -> bool {
    resolve_picoquic_libs(dir).is_some()
}

fn resolve_picoquic_libs_single_dir(dir: &Path) -> Option<Vec<&'static str>> {
    const REQUIRED: [(&str, &str); 4] = [
        ("picoquic_core", "picoquic-core"),
        ("picotls_core", "picotls-core"),
        ("picotls_openssl", "picotls-openssl"),
        ("picotls_minicrypto", "picotls-minicrypto"),
    ];
    let mut libs = Vec::with_capacity(REQUIRED.len() + 1);
    for (underscored, hyphenated) in REQUIRED {
        libs.push(find_lib_variant(dir, underscored, hyphenated)?);
    }
    if let Some(fusion) = find_lib_variant(dir, "picotls_fusion", "picotls-fusion") {
        libs.insert(3, fusion);
    }
    Some(libs)
}

fn resolve_picoquic_libs_split(
    picoquic_dir: &Path,
    picotls_dir: &Path,
) -> Option<Vec<&'static str>> {
    let picoquic_core = find_lib_variant(picoquic_dir, "picoquic_core", "picoquic-core")?;
    let picotls_core = find_lib_variant(picotls_dir, "picotls_core", "picotls-core")?;
    let picotls_minicrypto =
        find_lib_variant(picotls_dir, "picotls_minicrypto", "picotls-minicrypto")?;
    let picotls_openssl = find_lib_variant(picotls_dir, "picotls_openssl", "picotls-openssl")?;
    let mut libs = vec![picoquic_core, picotls_core, picotls_openssl];
    if let Some(fusion) = find_lib_variant(picotls_dir, "picotls_fusion", "picotls-fusion") {
        libs.push(fusion);
    }
    libs.push(picotls_minicrypto);
    Some(libs)
}

fn find_lib_variant<'a>(dir: &Path, underscored: &'a str, hyphenated: &'a str) -> Option<&'a str> {
    // Unix (.a)
    let underscored_path = dir.join(format!("lib{}.a", underscored));
    if underscored_path.exists() {
        return Some(underscored);
    }
    let hyphen_path = dir.join(format!("lib{}.a", hyphenated));
    if hyphen_path.exists() {
        return Some(hyphenated);
    }
    // Windows/MSVC (.lib)
    let underscored_lib = dir.join(format!("{}.lib", underscored));
    if underscored_lib.exists() {
        return Some(underscored);
    }
    let hyphen_lib = dir.join(format!("{}.lib", hyphenated));
    if hyphen_lib.exists() {
        return Some(hyphenated);
    }
    // MSVC Release subdirectory
    let release_dir = dir.join("Release");
    if release_dir.is_dir() {
        if release_dir.join(format!("{}.lib", underscored)).exists() {
            return Some(underscored);
        }
        if release_dir.join(format!("{}.lib", hyphenated)).exists() {
            return Some(hyphenated);
        }
    }
    None
}
