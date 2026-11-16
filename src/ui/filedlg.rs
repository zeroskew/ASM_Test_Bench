//! Native Windows open / save dialogs (comdlg32), without extra crates.

use std::path::{Path, PathBuf};

#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct OpenFileNameW {
        lStructSize: u32,
        hwndOwner: isize,
        hInstance: isize,
        lpstrFilter: *const u16,
        lpstrCustomFilter: *mut u16,
        nMaxCustFilter: u32,
        nFilterIndex: u32,
        lpstrFile: *mut u16,
        nMaxFile: u32,
        lpstrFileTitle: *mut u16,
        nMaxFileTitle: u32,
        lpstrInitialDir: *const u16,
        lpstrTitle: *const u16,
        Flags: u32,
        nFileOffset: u16,
        nFileExtension: u16,
        lpstrDefExt: *const u16,
        lCustData: isize,
        lpfnHook: *const c_void,
        lpTemplateName: *const u16,
        pvReserved: *const c_void,
        dwReserved: u32,
        FlagsEx: u32,
    }

    #[link(name = "comdlg32")]
    extern "system" {
        fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> i32;
        fn GetSaveFileNameW(ofn: *mut OpenFileNameW) -> i32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetActiveWindow() -> isize;
    }

    const OFN_OVERWRITEPROMPT: u32 = 0x2;
    const OFN_NOCHANGEDIR: u32 = 0x8;
    const OFN_PATHMUSTEXIST: u32 = 0x800;
    const OFN_FILEMUSTEXIST: u32 = 0x1000;
    const OFN_EXPLORER: u32 = 0x80000;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    pub fn dialog(save: bool, initial: Option<&Path>, title: Option<&str>) -> Option<PathBuf> {
        // Filter: pairs of NUL-terminated strings, ending with an extra NUL.
        let filter: Vec<u16> = "Assembly files (*.asm;*.s;*.inc)\0*.asm;*.s;*.inc\0All files (*.*)\0*.*\0\0".encode_utf16().collect();
        let mut file = vec![0u16; 1024];
        let mut dir: Vec<u16> = vec![0];
        if let Some(p) = initial {
            if let Some(name) = p.file_name() {
                let n: Vec<u16> = name.encode_wide().take(1000).collect();
                file[..n.len()].copy_from_slice(&n);
            }
            if let Some(d) = p.parent() {
                dir = d.as_os_str().encode_wide().chain([0]).collect();
            }
        }
        let def_ext = wide("asm");
        let title = wide(title.unwrap_or(if save { "Save assembly file" } else { "Open assembly file" }));
        let mut ofn = OpenFileNameW {
            lStructSize: std::mem::size_of::<OpenFileNameW>() as u32,
            hwndOwner: unsafe { GetActiveWindow() },
            hInstance: 0,
            lpstrFilter: filter.as_ptr(),
            lpstrCustomFilter: std::ptr::null_mut(),
            nMaxCustFilter: 0,
            nFilterIndex: 1,
            lpstrFile: file.as_mut_ptr(),
            nMaxFile: file.len() as u32,
            lpstrFileTitle: std::ptr::null_mut(),
            nMaxFileTitle: 0,
            lpstrInitialDir: if dir.len() > 1 { dir.as_ptr() } else { std::ptr::null() },
            lpstrTitle: title.as_ptr(),
            Flags: OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | if save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST },
            nFileOffset: 0,
            nFileExtension: 0,
            lpstrDefExt: def_ext.as_ptr(),
            lCustData: 0,
            lpfnHook: std::ptr::null(),
            lpTemplateName: std::ptr::null(),
            pvReserved: std::ptr::null(),
            dwReserved: 0,
            FlagsEx: 0,
        };
        let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
        if ok == 0 {
            return None;
        }
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        Some(PathBuf::from(String::from_utf16_lossy(&file[..len])))
    }
}

#[cfg(windows)]
pub fn open(initial: Option<&Path>) -> Option<PathBuf> {
    imp::dialog(false, initial, None)
}

#[cfg(windows)]
pub fn save(initial: Option<&Path>) -> Option<PathBuf> {
    imp::dialog(true, initial, None)
}

/// Save dialog with a custom window title (e.g. to say why it is being shown).
#[cfg(windows)]
pub fn save_titled(initial: Option<&Path>, title: &str) -> Option<PathBuf> {
    imp::dialog(true, initial, Some(title))
}

#[cfg(not(windows))]
pub fn open(_: Option<&Path>) -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
pub fn save(_: Option<&Path>) -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
pub fn save_titled(_: Option<&Path>, _: &str) -> Option<PathBuf> {
    None
}
