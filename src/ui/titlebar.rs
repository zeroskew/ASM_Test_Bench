//! Dark Windows title bar.
//!
//! `ViewportCommand::SetTheme(Dark)` alone doesn't stick on machines where apps use the light
//! theme: winit re-applies the system theme shortly after start. So we also set the caption and
//! caption-text colours directly through DWM (Windows 11), which winit never touches.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    type Hwnd = isize;

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(cb: extern "system" fn(Hwnd, isize) -> i32, lparam: isize) -> i32;
        fn GetWindowThreadProcessId(h: Hwnd, pid: *mut u32) -> u32;
        fn IsWindowVisible(h: Hwnd) -> i32;
        fn GetWindowTextW(h: Hwnd, buf: *mut u16, max: i32) -> i32;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(h: Hwnd, attr: u32, value: *const c_void, size: u32) -> i32;
    }

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_TEXT_COLOR: u32 = 36;

    struct Search {
        pid: u32,
        found: Vec<Hwnd>,
    }

    extern "system" fn cb(h: Hwnd, lparam: isize) -> i32 {
        unsafe {
            let s = &mut *(lparam as *mut Search);
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if pid == s.pid && IsWindowVisible(h) != 0 {
                let mut buf = [0u16; 256];
                let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
                if String::from_utf16_lossy(&buf[..n.max(0) as usize]).contains("ASM Test Bench") {
                    s.found.push(h);
                }
            }
            1
        }
    }

    /// Applies dark caption colours to this process's main window. Returns false until the
    /// window exists (call again on a later frame).
    pub fn apply() -> bool {
        let mut s = Search { pid: std::process::id(), found: Vec::new() };
        unsafe {
            EnumWindows(cb, &mut s as *mut Search as isize);
        }
        for &h in &s.found {
            // COLORREF is 0x00BBGGRR; matches the app's panel colour.
            let caption: u32 = 0x00272221;
            let text: u32 = 0x00DCDCDC;
            let dark: i32 = 1;
            unsafe {
                DwmSetWindowAttribute(h, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const i32 as *const c_void, 4);
                DwmSetWindowAttribute(h, DWMWA_CAPTION_COLOR, &caption as *const u32 as *const c_void, 4);
                DwmSetWindowAttribute(h, DWMWA_TEXT_COLOR, &text as *const u32 as *const c_void, 4);
            }
        }
        !s.found.is_empty()
    }
}

#[cfg(windows)]
pub use imp::apply;

#[cfg(not(windows))]
pub fn apply() -> bool {
    true
}
