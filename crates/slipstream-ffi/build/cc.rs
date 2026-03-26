use std::path::{Path, PathBuf};

/// Compile multiple C source files with a shared include directory and link
/// them as a static library. Uses the `cc` crate for cross-platform support
/// (GCC, Clang, MSVC).
pub(crate) fn compile_cc_lib(name: &str, sources: &[PathBuf], include_dirs: &[&Path]) {
    let mut build = cc::Build::new();
    for source in sources {
        build.file(source);
    }
    for dir in include_dirs {
        build.include(dir);
    }
    build.pic(true);
    build.compile(name);
}
