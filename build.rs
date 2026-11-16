//! Build script:
//! - Embeds every `examples/*.asm`, `tutorial/*.asm` and `advanced/*.asm` into the exe (generates
//!   `$OUT_DIR/examples.rs`), so new examples and lessons only need to be dropped into the
//!   folder.
//! - Embeds the app icon (assets/asm_test_bench.ico): compiles assets/asm_test_bench.rc with the Windows
//!   SDK's rc.exe and hands the .res to the linker. rc.exe is on PATH under the VS developer
//!   environment (build.cmd); otherwise the newest SDK copy is used. Without one the exe still
//!   builds, just without an icon.
//! - Stamps the build version (ASM_TEST_BENCH_VERSION, UTC "YYYY.MM.DD-HHMM"; the release tag is
//!   "v" + this) and compiles the relauncher (tools/relauncher.rs) that src/update.rs embeds.

use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    if Command::new("rc.exe").arg("/?").output().is_ok_and(|o| o.status.success() || !o.stdout.is_empty()) {
        return Some(PathBuf::from("rc.exe"));
    }
    let kits = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(kits).ok()?.flatten().map(|e| e.path()).filter(|p| p.join(r"x64\rc.exe").exists()).collect();
    versions.sort();
    versions.pop().map(|v| v.join(r"x64\rc.exe"))
}

/// UTC now as "YYYY.MM.DD-HHMM" (or $ASM_TEST_BENCH_VERSION when set).
fn build_version() -> String {
    if let Ok(v) = std::env::var("ASM_TEST_BENCH_VERSION") {
        return v;
    }
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}.{m:02}.{d:02}-{:02}{:02}", rem / 3600, rem % 3600 / 60)
}

/// Compiles tools/relauncher.rs (std only) into OUT_DIR/relauncher.exe with the same rustc. On failure
/// an empty file is left, and the app reports the relauncher as missing.
fn build_relauncher() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("relauncher.exe");
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let target = std::env::var("TARGET").unwrap_or_default();
    let mut cmd = Command::new(rustc);
    cmd.args(["--edition=2021", "--crate-type=bin", "--crate-name=relauncher", "-O", "-C", "panic=abort", "-C", "target-feature=+crt-static", "-C", "debuginfo=0"]);
    if !target.is_empty() {
        cmd.args(["--target", &target]);
    }
    cmd.arg("-o").arg(&out).arg("tools/relauncher.rs");
    match cmd.status() {
        Ok(s) if s.success() => {}
        other => {
            println!("cargo:warning=couldn't build the relauncher ({other:?}); self-update is disabled");
            let _ = std::fs::write(&out, b"");
        }
    }
}

/// Appends `pub const <konst>: &[(file name, source)]` for every .asm file in `folder`.
fn embed_folder(code: &mut String, folder: &str, konst: &str, what: &str) {
    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join(folder);
    // The folder's own timestamp changes when files are added or removed; include_str! tracks
    // the contents of each file.
    println!("cargo:rerun-if-changed={folder}");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("asm"))).collect())
        .unwrap_or_default();
    files.sort();
    code.push_str(&format!("/// (file name, source) of every {what}, sorted by file name.\npub const {konst}: &[(&str, &str)] = &[\n"));
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy();
        // Relative to the crate root (not an absolute path), so a cached copy of this file
        // still works after the project folder is moved or renamed.
        code.push_str(&format!("    ({name:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{folder}/\", {name:?}))),\n"));
    }
    code.push_str("];\n");
}

fn embed_examples(out_dir: &Path) {
    let mut code = String::new();
    embed_folder(&mut code, "examples", "EMBEDDED", "example");
    embed_folder(&mut code, "tutorial", "TUTORIAL", "tutorial lesson");
    embed_folder(&mut code, "advanced", "ADVANCED", "advanced tutorial lesson");
    std::fs::write(out_dir.join("examples.rs"), code).unwrap();
}

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    embed_examples(&out_dir);

    // A new version stamp whenever the sources change.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=tools/relauncher.rs");
    println!("cargo:rerun-if-env-changed=ASM_TEST_BENCH_VERSION");
    println!("cargo:rustc-env=ASM_TEST_BENCH_VERSION={}", build_version());
    build_relauncher();

    println!("cargo:rerun-if-changed=assets/asm_test_bench.rc");
    println!("cargo:rerun-if-changed=assets/asm_test_bench.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe not found; building without the exe icon");
        return;
    };
    let out = out_dir.join("asm_test_bench.res");
    let status = Command::new(&rc).args(["/nologo", "/fo"]).arg(&out).arg("assets/asm_test_bench.rc").status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", out.display()),
        other => println!("cargo:warning=rc.exe failed ({other:?}); building without the exe icon"),
    }
}
