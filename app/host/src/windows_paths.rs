//! Private storage primitives for a native Windows Host.

#![cfg(windows)]

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x7;
const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
const REPLACEFILE_IGNORE_MERGE_ERRORS: u32 = 0x2;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    fn ReplaceFileW(
        replaced: *const u16,
        replacement: *const u16,
        backup: *const u16,
        flags: u32,
        excluded: *const std::ffi::c_void,
        reserved: *const std::ffi::c_void,
    ) -> i32;
    fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut HandleFileInfo) -> i32;
    fn GetFinalPathNameByHandleW(handle: *mut std::ffi::c_void, buffer: *mut u16, length: u32, flags: u32) -> u32;
}

pub const APP_SUPPORT_DIRECTORY: &str = "EasyCodexInput";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub root: PathBuf,
    pub dashscope_env: PathBuf,
    pub state_database: PathBuf,
    pub installation_id: PathBuf,
    pub device_secret: PathBuf,
    pub cache_secret: PathBuf,
    pub cache_directory: PathBuf,
    pub runtime_directory: PathBuf,
}

impl AppPaths {
    pub fn from_home(home: &Path) -> Self {
        Self::from_root(home.join("AppData").join("Local").join(APP_SUPPORT_DIRECTORY))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            dashscope_env: root.join(".env"),
            state_database: root.join("state.sqlite3"),
            installation_id: root.join("installation-id"),
            device_secret: root.join("device-secret.hex"),
            cache_secret: root.join("cache-secret.hex"),
            cache_directory: root.join("cache").join("tts"),
            runtime_directory: root.join("run"),
            root,
        }
    }

    pub fn prepare(&self) -> io::Result<()> {
        secure_directory(&self.root)?;
        secure_directory(&self.cache_directory)?;
        secure_directory(&self.runtime_directory)
    }
}

pub struct ExplicitFileLock(File);

impl ExplicitFileLock {
    pub fn from_locked(file: File) -> Self {
        Self(file)
    }

    pub fn set_len(&self, size: u64) -> io::Result<()> {
        self.0.set_len(size)
    }

    pub fn sync_all(&self) -> io::Result<()> {
        self.0.sync_all()
    }

    #[cfg(test)]
    pub fn file(&self) -> &File {
        &self.0
    }
}

impl Drop for ExplicitFileLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}

#[repr(C)]
#[derive(Default)]
struct HandleFileInfo {
    attributes: u32,
    created: FileTime,
    accessed: FileTime,
    written: FileTime,
    volume_serial: u32,
    file_size_high: u32,
    file_size_low: u32,
    links: u32,
    file_index_high: u32,
    file_index_low: u32,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn wide(path: &Path) -> io::Result<Vec<u16>> {
    let value: Vec<u16> = path.as_os_str().encode_wide().collect();
    if value.contains(&0) {
        return Err(invalid("path contains NUL"));
    }
    Ok(value.into_iter().chain(Some(0)).collect())
}

fn reject_reparse(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid("reparse point is not allowed in private storage"));
    }
    Ok(metadata)
}

pub fn secure_directory(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(invalid("private path must be absolute"));
    }
    for current in path.ancestors().filter(|part| part.is_absolute()).collect::<Vec<_>>().into_iter().rev() {
        let metadata = match reject_reparse(current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(current)?;
                reject_reparse(current)?
            }
            Err(error) => return Err(error),
        };
        if !metadata.is_dir() {
            return Err(invalid("private path contains a non-directory"));
        }
    }
    Ok(())
}

pub fn validate_directory(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(invalid("working directory must be absolute"));
    }
    for current in path.ancestors().filter(|part| part.is_absolute()) {
        if !reject_reparse(current)?.is_dir() {
            return Err(invalid("working directory contains a non-directory"));
        }
    }
    Ok(())
}

fn directory_file(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    if !file.metadata()?.is_dir()
        || file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(invalid("directory handle is not a regular directory"));
    }
    Ok(file)
}

pub fn open_owned_directory_chain(path: &Path, create_missing: bool) -> io::Result<File> {
    if create_missing {
        secure_directory(path)?;
    } else {
        validate_directory(path)?;
    }
    directory_file(path)
}

fn path_from_handle(directory: &File) -> io::Result<PathBuf> {
    // SAFETY: a zero-length query asks Windows for the required UTF-16 buffer size.
    let needed = unsafe {
        GetFinalPathNameByHandleW(directory.as_raw_handle(), std::ptr::null_mut(), 0, 0)
    };
    if needed == 0 || needed > 32_767 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0_u16; needed as usize + 1];
    // SAFETY: buffer is writable for its declared capacity and handle is live.
    let written = unsafe {
        GetFinalPathNameByHandleW(
            directory.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if written == 0 || written as usize >= buffer.len() {
        return Err(io::Error::last_os_error());
    }
    buffer.truncate(written as usize);
    let path = PathBuf::from(OsString::from_wide(&buffer));
    if !path.is_absolute() {
        return Err(invalid("directory handle did not resolve to an absolute path"));
    }
    Ok(path)
}

pub(crate) fn child_path(directory: &File, name: &OsStr) -> io::Result<PathBuf> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(invalid("child name must be one normal component"));
    }
    let parent = path_from_handle(directory)?;
    if !reject_reparse(&parent)?.is_dir() {
        return Err(invalid("directory handle path changed"));
    }
    Ok(parent.join(name))
}

pub fn open_file_at(directory: &File, name: &OsStr, create: bool) -> io::Result<File> {
    let path = child_path(directory, name)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    if create {
        options.write(true).create(true);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file()
        || file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(invalid("child is not a regular file"));
    }
    Ok(file)
}

pub fn directory_names_at(directory: &File) -> io::Result<Vec<OsString>> {
    let path = path_from_handle(directory)?;
    validate_directory(&path)?;
    fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect()
}

pub fn open_private_file(path: &Path) -> io::Result<File> {
    let parent = path.parent().ok_or_else(|| invalid("file has no parent"))?;
    secure_directory(parent)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid("private file is not a regular file"));
    }
    Ok(file)
}

pub fn read_private_file(path: &Path) -> io::Result<File> {
    let parent = path.parent().ok_or_else(|| invalid("file has no parent"))?;
    validate_directory(parent)?;
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid("private file is not a regular file"));
    }
    Ok(file)
}

fn file_info(file: &File) -> io::Result<HandleFileInfo> {
    let mut info = HandleFileInfo::default();
    // SAFETY: the file handle remains open, and info is writable for this call.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

pub fn file_identity(file: &File) -> io::Result<(u64, u64)> {
    let info = file_info(file)?;
    Ok((info.volume_serial as u64, ((info.file_index_high as u64) << 32) | info.file_index_low as u64))
}

pub fn same_file_handle(opened: &File, path: &Path) -> io::Result<bool> {
    if !reject_reparse(path)?.is_file() {
        return Ok(false);
    }
    let current = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let left = file_info(opened)?;
    let right = file_info(&current)?;
    Ok(left.volume_serial == right.volume_serial
        && left.file_index_high == right.file_index_high
        && left.file_index_low == right.file_index_low)
}

pub fn replace_private_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| invalid("file has no parent"))?;
    secure_directory(parent)?;
    match reject_reparse(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(invalid("private destination is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let name = path.file_name().ok_or_else(|| invalid("file has no name"))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid("system clock is unavailable"))?
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id(),
        timestamp,
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        file.rewind()?;
        let mut readback = Vec::new();
        (&file)
            .take(contents.len().saturating_add(1) as u64)
            .read_to_end(&mut readback)?;
        if readback != contents {
            readback.fill(0);
            return Err(invalid("private temporary readback mismatch"));
        }
        readback.fill(0);
        drop(file);
        let old = wide(&temporary)?;
        let new = wide(path)?;
        // SAFETY: both path buffers are NUL-terminated and live for the call.
        let replaced = if path.exists() {
            // SAFETY: both paths are NUL-terminated, point to files in one directory,
            // and the remaining optional pointers are null.
            unsafe {
                ReplaceFileW(
                    new.as_ptr(),
                    old.as_ptr(),
                    std::ptr::null(),
                    REPLACEFILE_IGNORE_MERGE_ERRORS,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            }
        } else {
            // SAFETY: both path buffers are NUL-terminated and live for the call.
            unsafe { MoveFileExW(old.as_ptr(), new.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) }
        };
        if replaced == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn rename_noreplace(source: &Path, target: &Path) -> io::Result<()> {
    let source_parent = source.parent().ok_or_else(|| invalid("source has no parent"))?;
    let target_parent = target.parent().ok_or_else(|| invalid("target has no parent"))?;
    validate_directory(source_parent)?;
    validate_directory(target_parent)?;
    if fs::symlink_metadata(target).is_ok() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "target exists"));
    }
    let source = wide(source)?;
    let target = wide(target)?;
    // MoveFileExW without MOVEFILE_REPLACE_EXISTING fails if another writer creates target.
    if unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
