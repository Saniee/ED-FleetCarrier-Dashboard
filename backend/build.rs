//! Embeds the EDMC plugin (`../edmc-plugin`) so the backend can serve it as a
//! zip (`/api/plugin/download`), always matching this build. Writes
//! `$OUT_DIR/plugin.rs` with the plugin's name, version and files.

use std::{env, fmt::Write, fs, path::Path};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../edmc-plugin");
    println!("cargo:rerun-if-changed={}", root.display());

    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    files.sort();

    let common = fs::read_to_string(root.join("edc/common.py")).expect("edmc-plugin/edc/common.py");
    let constant = |name: &str| {
        common
            .lines()
            .find_map(|l| l.strip_prefix(name)?.trim_start().strip_prefix('=')?.trim().strip_prefix('"')?.strip_suffix('"').map(str::to_owned))
            .unwrap_or_else(|| panic!("{name} = \"...\" not found in common.py"))
    };

    let mut out = String::new();
    writeln!(out, "pub const NAME: &str = {:?};", constant("PLUGIN_NAME")).unwrap();
    writeln!(out, "pub const VERSION: &str = {:?};", constant("__version__")).unwrap();
    writeln!(out, "pub const FILES: &[(&str, &[u8])] = &[").unwrap();
    for (rel, abs) in &files {
        writeln!(out, "    ({rel:?}, include_bytes!({abs:?})),").unwrap();
    }
    writeln!(out, "];").unwrap();

    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("plugin.rs"), out).unwrap();
}

/// Every file under `dir` as `(path relative to root with '/', absolute path)`,
/// leaving out Python caches.
fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).expect("readable edmc-plugin directory") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if name == "__pycache__" || name.ends_with(".pyc") {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, files);
        } else {
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            let abs = fs::canonicalize(&path).unwrap().to_string_lossy().into_owned();
            files.push((rel, abs));
        }
    }
}
