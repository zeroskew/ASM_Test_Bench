//! Self-update from GitHub Releases.
//!
//! The build stamps a version (build.rs: UTC "YYYY.MM.DD-HHMM"); the publish script tags the
//! release "v<version>". A check asks the GitHub API for the latest release; when its tag is newer,
//! its `asm_test_bench.zip` (or a bare `asm_test_bench.exe`) is downloaded into
//! %LOCALAPPDATA%\ASM_Test_Bench\update\ and the exe unpacked there. When the app exits with an update
//! ready ("Restart now", or any normal quit), main.rs calls [`apply_on_exit`]: it writes the
//! embedded relauncher (tools/relauncher.rs) next to the download and starts it; the relauncher waits for
//! this process to end, swaps the exe and, for "Restart now", starts it again.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const VERSION: &str = env!("ASM_TEST_BENCH_VERSION");
const REPO: &str = "zeroskew/ASM_Test_Bench";
const RELAUNCHER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/relauncher.exe"));
/// The start-up check runs this long after start-up (the app settles first).
const FIRST_CHECK: Duration = Duration::from_secs(20);

/// When automatic checks run.
pub struct Schedule {
    pub on_startup: bool,
    /// Every this many minutes (counted from start-up, then from the last automatic check).
    pub every_min: Option<u32>,
}

#[derive(Clone, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    /// The latest release is this one (or older): its tag.
    UpToDate(String),
    Downloading { tag: String, done: u64, total: u64 },
    /// Downloaded and unpacked; installs when the app exits.
    Ready(String),
    Failed(String),
}

static STATUS: Mutex<Status> = Mutex::new(Status::Idle);
/// The unpacked new exe, once ready.
static READY: Mutex<Option<PathBuf>> = Mutex::new(None);
static BUSY: AtomicBool = AtomicBool::new(false);
static LAST_AUTO: Mutex<Option<Instant>> = Mutex::new(None);
/// The exe this process was started from, recorded at start-up. There's no installer: each copy
/// of ASM Test Bench updates only its own exe, wherever it was launched from.
static LAUNCHED: OnceLock<PathBuf> = OnceLock::new();
/// "Restart now" was picked: start the app again after updating.
pub static RELAUNCH: AtomicBool = AtomicBool::new(false);

pub fn status() -> Status {
    STATUS.lock().unwrap().clone()
}

fn set(s: Status) {
    *STATUS.lock().unwrap() = s;
}

/// Where downloads go.
fn update_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("ASM_Test_Bench").join("update")
}

/// The exe an update replaces: the one this process was started from (recorded by [`init`]).
pub fn target_exe() -> Option<PathBuf> {
    LAUNCHED.get().cloned()
}

/// A build run from a cargo `target` folder: no automatic checks (a manual check still works).
pub fn is_dev_build() -> bool {
    target_exe().is_none_or(|p| p.components().any(|c| c.as_os_str().eq_ignore_ascii_case("target")))
}

/// "v2026.11.15-2342" → 202611152342, for comparing versions.
fn version_number(v: &str) -> u64 {
    v.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

/// At start-up: records which exe is running (the only file an update replaces) and removes
/// what earlier updates left behind (the relauncher, old downloads).
pub fn init() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = LAUNCHED.set(exe);
    }
    let dir = update_dir();
    std::thread::spawn(move || {
        // The relauncher may still be finishing after starting us again.
        std::thread::sleep(Duration::from_secs(5));
        if let Ok(rd) = std::fs::read_dir(relauncher_dir()) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("relauncher") && n.ends_with(".exe") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        if READY.lock().unwrap().is_some() {
            return;
        }
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if name.ends_with(".exe") || name.ends_with(".zip") || name.ends_with(".part") {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    });
}

/// Called every frame: starts an automatic check when one is due.
pub fn auto_check(ctx: &egui::Context, enabled: bool, schedule: &Schedule) {
    if !enabled || is_dev_build() || BUSY.load(Ordering::Relaxed) || READY.lock().unwrap().is_some() {
        return;
    }
    let every = schedule.every_min.map(|m| Duration::from_secs(m.max(1) as u64 * 60));
    let mut last = LAST_AUTO.lock().unwrap();
    // How long until the next automatic check (zero = due now; None = none planned).
    let wait = match *last {
        Some(t) => every.map(|e| e.saturating_sub(t.elapsed())),
        None => {
            let up = Duration::from_secs_f64(ctx.input(|i| i.time).max(0.0));
            let startup = schedule.on_startup.then(|| FIRST_CHECK.saturating_sub(up));
            let periodic = every.map(|e| e.saturating_sub(up));
            startup.into_iter().chain(periodic).min()
        }
    };
    match wait {
        Some(w) if w.is_zero() => {
            *last = Some(Instant::now());
            drop(last);
            check(ctx);
        }
        // Wake up for it even if nothing else repaints by then.
        Some(w) => ctx.request_repaint_after(w),
        None => {}
    }
}

/// Checks for (and downloads) a newer release on a thread. Nothing if one is running or an update
/// is already downloaded.
pub fn check(ctx: &egui::Context) {
    if READY.lock().unwrap().is_some() || BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    set(Status::Checking);
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = run_check(&ctx);
        set(match r {
            Ok(s) => s,
            Err(e) => Status::Failed(e),
        });
        BUSY.store(false, Ordering::SeqCst);
        ctx.request_repaint();
    });
}

fn run_check(ctx: &egui::Context) -> Result<Status, String> {
    if RELAUNCHER.is_empty() {
        return Err("This build has no relauncher (it couldn't be compiled).".into());
    }
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = match http::get(&url, "application/vnd.github+json", &mut |_, _| {}) {
        Ok(b) => b,
        Err(http::Error::Status(404)) => return Err("No release is published yet.".into()),
        Err(http::Error::Status(403 | 429)) => return Err("GitHub's rate limit was hit; try again later.".into()),
        Err(e) => return Err(e.to_string()),
    };
    let json: serde_json::Value = serde_json::from_slice(&body).map_err(|e| format!("Unexpected reply from GitHub: {e}"))?;
    let tag = json["tag_name"].as_str().ok_or("The latest release has no tag.")?.to_string();
    if version_number(&tag) <= version_number(VERSION) {
        return Ok(Status::UpToDate(tag));
    }
    let assets = json["assets"].as_array().cloned().unwrap_or_default();
    let find = |name: &str| assets.iter().find(|a| a["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name)));
    let (asset, zipped) = match (find("asm_test_bench.exe"), find("asm_test_bench.zip")) {
        (Some(a), _) => (a, false),
        (None, Some(a)) => (a, true),
        _ => return Err(format!("Release {tag} has no asm_test_bench.zip.")),
    };
    let dl = asset["browser_download_url"].as_str().ok_or("The release asset has no download link.")?.to_string();
    let size = asset["size"].as_u64().unwrap_or(0);

    set(Status::Downloading { tag: tag.clone(), done: 0, total: size });
    ctx.request_repaint();
    let mut last = Instant::now();
    let data = http::get(&dl, "application/octet-stream", &mut |done, total| {
        if last.elapsed() > Duration::from_millis(100) {
            last = Instant::now();
            set(Status::Downloading { tag: tag.clone(), done, total: if total > 0 { total } else { size } });
            ctx.request_repaint();
        }
    })
    .map_err(|e| format!("Download failed: {e}"))?;
    let exe = if zipped { zip::extract(&data, "asm_test_bench.exe")? } else { data };
    if exe.len() < 1 << 20 || !exe.starts_with(b"MZ") {
        return Err("The downloaded file isn't a valid ASM Test Bench executable.".into());
    }
    let dir = update_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    let path = dir.join(format!("asm_test_bench-{}.exe", tag.trim_start_matches('v')));
    let part = path.with_extension("part");
    std::fs::write(&part, &exe).and_then(|_| std::fs::rename(&part, &path)).map_err(|e| format!("Couldn't save the update: {e}"))?;
    *READY.lock().unwrap() = Some(path);
    Ok(Status::Ready(tag))
}

/// At exit (after the window closed): starts the relauncher when an update is ready.
pub fn apply_on_exit() {
    let Some(new) = READY.lock().unwrap().clone() else { return };
    let Some(target) = target_exe() else { return };
    let Some(relauncher) = write_relauncher(&relauncher_dir()) else { return };
    let mut cmd = std::process::Command::new(&relauncher);
    cmd.arg(&new).arg(&target);
    if RELAUNCH.load(Ordering::Relaxed) {
        cmd.arg("--relaunch");
    }
    if let Err(e) = cmd.spawn() {
        eprintln!("couldn't start the relauncher: {e}");
    }
}

/// %TEMP%\ASM_Test_Bench: the relauncher runs from here.
fn relauncher_dir() -> PathBuf {
    std::env::temp_dir().join("ASM_Test_Bench")
}

fn write_relauncher(dir: &Path) -> Option<PathBuf> {
    let _ = std::fs::create_dir_all(dir);
    // A copy may still be running from the last update; then use another name.
    for name in ["relauncher.exe".to_string(), format!("relauncher-{}.exe", std::process::id())] {
        let p = dir.join(name);
        if std::fs::write(&p, RELAUNCHER).is_ok() {
            return Some(p);
        }
    }
    None
}

/// "2.4 MB"
pub fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1048576.0)
}

mod zip {
    /// One file out of a zip (stored or deflated), found by name through the central directory.
    pub fn extract(data: &[u8], want: &str) -> Result<Vec<u8>, String> {
        let bad = || "The downloaded zip is damaged.".to_string();
        let u16_at = |o: usize| data.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or_else(bad);
        let u32_at = |o: usize| data.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or_else(bad);
        // End of central directory: the last "PK\5\6".
        let eocd = (0..data.len().saturating_sub(21)).rev().find(|&i| data[i..].starts_with(b"PK\x05\x06")).ok_or_else(bad)?;
        let entries = u16_at(eocd + 10)?;
        let mut p = u32_at(eocd + 16)?;
        for _ in 0..entries {
            if !data.get(p..).is_some_and(|d| d.starts_with(b"PK\x01\x02")) {
                return Err(bad());
            }
            let method = u16_at(p + 10)?;
            let csize = u32_at(p + 20)?;
            let usize_ = u32_at(p + 24)?;
            let (nlen, xlen, clen) = (u16_at(p + 28)?, u16_at(p + 30)?, u16_at(p + 32)?);
            let local = u32_at(p + 42)?;
            let name = String::from_utf8_lossy(data.get(p + 46..p + 46 + nlen).ok_or_else(bad)?).to_string();
            p += 46 + nlen + xlen + clen;
            let base = name.rsplit(['/', '\\']).next().unwrap_or(&name);
            if !base.eq_ignore_ascii_case(want) {
                continue;
            }
            if !data.get(local..).is_some_and(|d| d.starts_with(b"PK\x03\x04")) {
                return Err(bad());
            }
            let start = local + 30 + u16_at(local + 26)? + u16_at(local + 28)?;
            let raw = data.get(start..start + csize).ok_or_else(bad)?;
            let out = match method {
                0 => raw.to_vec(),
                8 => miniz_oxide::inflate::decompress_to_vec(raw).map_err(|_| bad())?,
                m => return Err(format!("The zip uses an unsupported compression method ({m}).")),
            };
            if out.len() != usize_ {
                return Err(bad());
            }
            return Ok(out);
        }
        Err(format!("{want} isn't in the downloaded zip."))
    }
}

mod http {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Networking::WinHttp::*;

    #[derive(Debug)]
    pub enum Error {
        Status(u32),
        Net(String),
    }

    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            match self {
                Error::Status(s) => write!(f, "the server answered HTTP {s}"),
                Error::Net(s) => write!(f, "{s}"),
            }
        }
    }

    struct Handle(*mut core::ffi::c_void);
    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn net(what: &str) -> Error {
        let e = windows::core::Error::from_thread();
        let msg = match e.code().0 as u32 & 0xffff {
            12002 => "the connection timed out".to_string(),
            12007 => "the server name couldn't be resolved (offline?)".to_string(),
            12029 | 12030 => "couldn't connect to the server".to_string(),
            12175 => "a secure connection couldn't be made".to_string(),
            _ => e.message(),
        };
        Error::Net(format!("Network error ({what}): {msg}"))
    }

    /// GET over HTTPS (redirects followed); `progress(done, total)` while reading the body.
    pub fn get(url: &str, accept: &str, progress: &mut dyn FnMut(u64, u64)) -> Result<Vec<u8>, Error> {
        let rest = url.strip_prefix("https://").ok_or_else(|| Error::Net(format!("not an https URL: {url}")))?;
        let (host, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{p}"))).unwrap_or((rest, "/".into()));
        unsafe {
            let session = Handle(WinHttpOpen(w!("ASM-Test-Bench-Updater"), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0));
            if session.0.is_null() {
                return Err(net("open"));
            }
            let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 30_000);
            let conn = Handle(WinHttpConnect(session.0, &HSTRING::from(host), 443, 0));
            if conn.0.is_null() {
                return Err(net("connect"));
            }
            let req = Handle(WinHttpOpenRequest(conn.0, w!("GET"), &HSTRING::from(path.as_str()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE));
            if req.0.is_null() {
                return Err(net("request"));
            }
            let headers: Vec<u16> = format!("Accept: {accept}\r\nX-GitHub-Api-Version: 2022-11-28\r\n").encode_utf16().collect();
            WinHttpSendRequest(req.0, Some(&headers), None, 0, 0, 0).map_err(|_| net("send"))?;
            WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|_| net("response"))?;
            let number = |level: u32| -> Option<u64> {
                let mut v: u32 = 0;
                let mut len = 4u32;
                let mut idx = 0u32;
                WinHttpQueryHeaders(req.0, level | WINHTTP_QUERY_FLAG_NUMBER, PCWSTR::null(), Some(&mut v as *mut u32 as _), &mut len, &mut idx).ok().map(|_| v as u64)
            };
            let status = number(WINHTTP_QUERY_STATUS_CODE).unwrap_or(0);
            if status != 200 {
                return Err(Error::Status(status as u32));
            }
            let total = number(WINHTTP_QUERY_CONTENT_LENGTH).unwrap_or(0);
            let mut out = Vec::with_capacity(total as usize);
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let mut read = 0u32;
                WinHttpReadData(req.0, buf.as_mut_ptr() as _, buf.len() as u32, &mut read).map_err(|_| net("read"))?;
                if read == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..read as usize]);
                progress(out.len() as u64, total);
            }
            if total > 0 && (out.len() as u64) < total {
                return Err(Error::Net("the download was cut short".into()));
            }
            Ok(out)
        }
    }
}
