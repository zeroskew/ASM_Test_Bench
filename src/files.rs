//! Files for the emulated program (open / read / write / ... system calls), kept inside one
//! folder: the program can't reach anything outside it. Paths use / or \, ".." and drive
//! letters are refused. Errors are Linux errno numbers (the system call returns -errno).

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

pub const ENOENT: i32 = 2;
pub const EIO: i32 = 5;
pub const EBADF: i32 = 9;
pub const EACCES: i32 = 13;
pub const EEXIST: i32 = 17;
pub const ENOTDIR: i32 = 20;
pub const EISDIR: i32 = 21;
pub const EINVAL: i32 = 22;
pub const EMFILE: i32 = 24;
pub const ENAMETOOLONG: i32 = 36;
pub const ENOTEMPTY: i32 = 39;

/// Flags of the open system call (Linux values).
pub const O_WRONLY: u32 = 0o1;
pub const O_RDWR: u32 = 0o2;
pub const O_CREAT: u32 = 0o100;
pub const O_EXCL: u32 = 0o200;
pub const O_TRUNC: u32 = 0o1000;
pub const O_APPEND: u32 = 0o2000;

/// Most files open at once.
const MAX_OPEN: usize = 16;
/// Largest single read / write.
pub const MAX_IO: usize = 1 << 20;

/// Name of an errno value, for messages ("ENOENT").
pub fn errno_name(e: i32) -> &'static str {
    match e {
        ENOENT => "ENOENT (no such file or folder)",
        EIO => "EIO (input/output error)",
        EBADF => "EBADF (not an open file)",
        EACCES => "EACCES (outside the program's folder, or not allowed)",
        EEXIST => "EEXIST (already exists)",
        ENOTDIR => "ENOTDIR (not a folder)",
        EISDIR => "EISDIR (is a folder)",
        EINVAL => "EINVAL (invalid argument)",
        EMFILE => "EMFILE (too many open files)",
        ENAMETOOLONG => "ENAMETOOLONG (name too long)",
        ENOTEMPTY => "ENOTEMPTY (folder not empty)",
        _ => "error",
    }
}

fn errno(e: &std::io::Error) -> i32 {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => ENOENT,
        PermissionDenied => EACCES,
        AlreadyExists => EEXIST,
        InvalidInput | InvalidData => EINVAL,
        NotADirectory => ENOTDIR,
        IsADirectory => EISDIR,
        DirectoryNotEmpty => ENOTEMPTY,
        _ => EIO,
    }
}

struct OpenFile {
    file: File,
    read: bool,
    write: bool,
}

/// The program's files: a root folder and the open file table (fd 3 and up; 0-2 are the
/// console).
pub struct Sandbox {
    root: Option<PathBuf>,
    open: BTreeMap<u32, OpenFile>,
}

pub type R<T> = Result<T, i32>;

impl Sandbox {
    /// `root`: the folder the program may use (None: no file access at all).
    pub fn new(root: Option<PathBuf>) -> Sandbox {
        Sandbox { root, open: BTreeMap::new() }
    }


    /// A program path inside the root folder.
    fn resolve(&self, path: &[u8]) -> R<PathBuf> {
        let root = self.root.as_ref().ok_or(EACCES)?;
        // The folder is made when the program first uses a file (Documents\ASM Test Bench).
        if !root.is_dir() {
            std::fs::create_dir_all(root).map_err(|e| errno(&e))?;
        }
        let text = std::str::from_utf8(path).map_err(|_| EINVAL)?;
        if text.is_empty() {
            return Err(ENOENT);
        }
        if text.len() > 255 {
            return Err(ENAMETOOLONG);
        }
        let mut p = root.clone();
        for part in text.split(['/', '\\']) {
            match part {
                "" | "." => {}
                ".." => return Err(EACCES),
                _ if part.contains(':') => return Err(EACCES), // drive letters, streams
                _ => p.push(part),
            }
        }
        Ok(p)
    }

    pub fn open(&mut self, path: &[u8], flags: u32) -> R<u32> {
        let p = self.resolve(path)?;
        if self.open.len() >= MAX_OPEN {
            return Err(EMFILE);
        }
        let access = flags & 3;
        let (read, write) = (access != O_WRONLY, access == O_WRONLY || access == O_RDWR);
        if p.is_dir() {
            return Err(EISDIR);
        }
        let mut o = OpenOptions::new();
        o.read(read).write(write || flags & O_APPEND != 0).append(flags & O_APPEND != 0);
        if flags & O_CREAT != 0 {
            if let Some(parent) = p.parent() {
                if !parent.is_dir() {
                    return Err(ENOENT);
                }
            }
            if flags & O_EXCL != 0 {
                o.create_new(true);
            } else {
                o.create(true);
            }
            o.write(true);
        }
        if flags & O_TRUNC != 0 && write {
            o.truncate(true);
        }
        let file = o.open(&p).map_err(|e| errno(&e))?;
        let fd = (3..).find(|fd| !self.open.contains_key(fd)).unwrap();
        self.open.insert(fd, OpenFile { file, read, write: write || flags & O_APPEND != 0 });
        Ok(fd)
    }

    pub fn close(&mut self, fd: u32) -> R<()> {
        if fd <= 2 {
            return Ok(()); // the console stays
        }
        self.open.remove(&fd).map(|_| ()).ok_or(EBADF)
    }


    pub fn read(&mut self, fd: u32, max: usize) -> R<Vec<u8>> {
        let f = self.open.get_mut(&fd).ok_or(EBADF)?;
        if !f.read {
            return Err(EBADF);
        }
        let mut buf = vec![0u8; max.min(MAX_IO)];
        let n = f.file.read(&mut buf).map_err(|e| errno(&e))?;
        buf.truncate(n);
        Ok(buf)
    }

    pub fn write(&mut self, fd: u32, data: &[u8]) -> R<usize> {
        let f = self.open.get_mut(&fd).ok_or(EBADF)?;
        if !f.write {
            return Err(EBADF);
        }
        f.file.write_all(data).map_err(|e| errno(&e))?;
        Ok(data.len())
    }

    /// whence: 0 = from the start, 1 = from the current position, 2 = from the end.
    pub fn lseek(&mut self, fd: u32, offset: i32, whence: u32) -> R<u32> {
        let f = self.open.get_mut(&fd).ok_or(EBADF)?;
        let pos = match whence {
            0 if offset >= 0 => SeekFrom::Start(offset as u64),
            1 => SeekFrom::Current(offset as i64),
            2 => SeekFrom::End(offset as i64),
            _ => return Err(EINVAL),
        };
        let p = f.file.seek(pos).map_err(|_| EINVAL)?;
        u32::try_from(p).map_err(|_| EINVAL)
    }

    pub fn unlink(&mut self, path: &[u8]) -> R<()> {
        let p = self.resolve(path)?;
        if p.is_dir() {
            return Err(EISDIR);
        }
        std::fs::remove_file(&p).map_err(|e| errno(&e))
    }

    pub fn rename(&mut self, from: &[u8], to: &[u8]) -> R<()> {
        let (a, b) = (self.resolve(from)?, self.resolve(to)?);
        std::fs::rename(&a, &b).map_err(|e| errno(&e))
    }

    pub fn mkdir(&mut self, path: &[u8]) -> R<()> {
        let p = self.resolve(path)?;
        std::fs::create_dir(&p).map_err(|e| errno(&e))
    }

    pub fn rmdir(&mut self, path: &[u8]) -> R<()> {
        let p = self.resolve(path)?;
        if !p.is_dir() {
            return Err(if p.exists() { ENOTDIR } else { ENOENT });
        }
        std::fs::remove_dir(&p).map_err(|e| errno(&e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("atb_files_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn stays_inside_its_folder() {
        let root = temp_root("sandbox");
        let mut s = Sandbox::new(Some(root.clone()));
        assert_eq!(s.open(b"../escape.txt", O_CREAT | O_WRONLY), Err(EACCES));
        assert_eq!(s.open(b"sub/../../x", O_CREAT | O_WRONLY), Err(EACCES));
        assert_eq!(s.open(b"C:\\Windows\\win.ini", 0), Err(EACCES));
        assert_eq!(s.open(b"", 0), Err(ENOENT));
        // "/name" means the root folder, not the disk.
        let fd = s.open(b"/top.txt", O_CREAT | O_WRONLY | O_TRUNC).unwrap();
        s.close(fd).unwrap();
        assert!(root.join("top.txt").exists());
        let mut none = Sandbox::new(None);
        assert_eq!(none.open(b"a.txt", 0), Err(EACCES));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_write_read_seek_append_delete() {
        let root = temp_root("rw");
        let mut s = Sandbox::new(Some(root.clone()));
        let fd = s.open(b"notes.txt", O_CREAT | O_WRONLY | O_TRUNC).unwrap();
        assert_eq!(fd, 3);
        assert_eq!(s.write(fd, b"hello"), Ok(5));
        assert_eq!(s.read(fd, 10), Err(EBADF), "opened write-only");
        s.close(fd).unwrap();
        assert_eq!(s.close(fd), Err(EBADF));
        let fd = s.open(b"notes.txt", O_WRONLY | O_APPEND).unwrap();
        s.write(fd, b" world").unwrap();
        s.close(fd).unwrap();
        let fd = s.open(b"notes.txt", 0).unwrap();
        assert_eq!(s.lseek(fd, 0, 2), Ok(11), "size");
        assert_eq!(s.lseek(fd, 6, 0), Ok(6));
        assert_eq!(s.read(fd, 100).unwrap(), b"world");
        assert_eq!(s.read(fd, 100).unwrap(), b"", "end of file");
        s.close(fd).unwrap();
        assert_eq!(s.open(b"notes.txt", O_CREAT | O_EXCL | O_WRONLY), Err(EEXIST));
        s.mkdir(b"dir").unwrap();
        s.rename(b"notes.txt", b"dir/n.txt").unwrap();
        assert_eq!(s.rmdir(b"dir"), Err(ENOTEMPTY));
        s.unlink(b"dir/n.txt").unwrap();
        s.rmdir(b"dir").unwrap();
        assert_eq!(s.unlink(b"nothing.txt"), Err(ENOENT));
        let _ = std::fs::remove_dir_all(&root);
    }
}
