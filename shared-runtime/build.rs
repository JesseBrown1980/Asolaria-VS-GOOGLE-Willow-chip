use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=ASOLARIA_TRIBIT_SOURCE");
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let source = env::var_os("ASOLARIA_TRIBIT_SOURCE")
        .map(PathBuf::from)
        .or_else(|| {
            [
                manifest.join("../canonical/asolaria-os/kernel/core/src/tribit/mod.rs"),
                PathBuf::from("C:/asolaria-acer/asolaria-os/kernel/core/src/tribit/mod.rs"),
                PathBuf::from("/mnt/c/asolaria-acer/asolaria-os/kernel/core/src/tribit/mod.rs"),
            ]
            .into_iter()
            .find(|path| path.is_file())
        })
        .expect("canonical tribit source required; set ASOLARIA_TRIBIT_SOURCE");
    assert!(source.is_file(), "canonical tribit source must be a file");
    println!("cargo:rerun-if-changed={}", source.display());
    let source = source.to_str().expect("UTF-8 canonical source path");
    let declaration = format!("#[allow(dead_code)]\n#[path = {source:?}]\nmod tribit;\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("build output"));
    fs::write(output.join("canonical_tribit.rs"), declaration).expect("write source pointer");
}
