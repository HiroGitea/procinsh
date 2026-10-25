//! Acquire ELF bytes from verified regular files without parsing ELF or DWARF.
//!
//! # Interface
//!
//! [`acquire`] (`pub(super)`): `fn acquire(pid: i32, map: &MemoryMap, known_file: Option<procinsh_protocol::analysis::snapshot::symbol::ElfFileId>) -> anyhow::Result<procinsh_protocol::ipc::schema::MonitorResponse>`.
//! Input mappings use [`MemoryMap`]; acquired bytes use [`ElfFile`].

use crate::privileged_helper::process::maps::MemoryMap;
use procinsh_protocol::analysis::snapshot::symbol::ElfFile;
use std::{
    fs,
    io::Read,
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::PathBuf,
};
const MAX_FILE_SIZE: u64 = 128 * 1024 * 1024;
fn file_identity(meta: &fs::Metadata) -> (u64, u64, u64, i64, i64) {
    (
        meta.dev(),
        meta.ino(),
        meta.len(),
        meta.mtime(),
        meta.mtime_nsec(),
    )
}
fn matching_file(pid: i32, map: &MemoryMap) -> Option<(fs::File, fs::Metadata)> {
    let mut paths = vec![
        PathBuf::from(format!(
            "/proc/{pid}/map_files/{:x}-{:x}",
            map.start, map.end
        )),
        PathBuf::from(format!("/proc/{pid}/exe")),
    ];
    if let Some(path) = map
        .pathname
        .as_deref()
        .filter(|p| p.starts_with('/') && !p.ends_with(" (deleted)"))
    {
        paths.push(PathBuf::from(format!("/proc/{pid}/root{path}")));
    }
    paths.into_iter().find_map(|path| {
        // O_PATH pins the selected inode without opening devices or blocking on
        // FIFOs. Only verified regular files may then be opened for reading.
        let pinned = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
            .open(&path)
            .ok()?;
        let meta = pinned.metadata().ok()?;
        if !meta.is_file()
            || meta.ino() != map.inode
            || libc::major(meta.dev()) != map.device.major
            || libc::minor(meta.dev()) != map.device.minor
        {
            return None;
        }
        let descriptor = fs::File::open(format!("/proc/self/fd/{}", pinned.as_raw_fd())).ok()?;
        (file_identity(&descriptor.metadata().ok()?) == file_identity(&meta))
            .then_some((descriptor, meta))
    })
}

pub(super) fn acquire(
    pid: i32,
    map: &MemoryMap,
    known_file: Option<procinsh_protocol::analysis::snapshot::symbol::ElfFileId>,
) -> anyhow::Result<procinsh_protocol::ipc::schema::MonitorResponse> {
    use procinsh_protocol::ipc::schema::MonitorResponse;
    let Some((mut descriptor, before)) = matching_file(pid, map) else {
        return Ok(MonitorResponse::Elf(None));
    };
    if before.len() > MAX_FILE_SIZE {
        return Ok(MonitorResponse::Elf(None));
    }
    let identity = file_identity(&before);
    if known_file == Some(identity) {
        return Ok(MonitorResponse::ElfUnchanged);
    }
    let mut bytes = Vec::new();
    (&mut descriptor)
        .take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut bytes)?;
    if identity != file_identity(&descriptor.metadata()?) || bytes.len() as u64 != before.len() {
        return Ok(MonitorResponse::Elf(None));
    }
    Ok(MonitorResponse::Elf(Some(ElfFile { identity, bytes })))
}
#[cfg(test)]
pub(in crate::privileged_helper::process) fn read(pid: i32, map: &MemoryMap) -> Option<ElfFile> {
    match acquire(pid, map, None).ok()? {
        procinsh_protocol::ipc::schema::MonitorResponse::Elf(file) => file,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verifies_identity_pins_regular_files_and_bounds_reads() {
        let root =
            std::env::temp_dir().join(format!("procinsh-elf-acquisition-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("mapped");
        fs::write(&path, b"captured file").unwrap();
        let metadata = fs::metadata(&path).unwrap();
        let map = crate::privileged_helper::process::maps::parse_map(&format!(
            "1000-2000 r-xp 0 {:x}:{:x} {} {}",
            libc::major(metadata.dev()),
            libc::minor(metadata.dev()),
            metadata.ino(),
            path.display()
        ))
        .unwrap();
        let pid = std::process::id() as i32;
        let captured = read(pid, &map).unwrap();
        assert_eq!(captured.bytes, b"captured file");
        assert!(matches!(
            acquire(pid, &map, Some(captured.identity)).unwrap(),
            procinsh_protocol::ipc::schema::MonitorResponse::ElfUnchanged
        ));
        let (mut descriptor, _) = matching_file(pid, &map).unwrap();
        fs::rename(&path, root.join("pinned")).unwrap();
        fs::write(&path, b"replacement").unwrap();
        assert!(read(pid, &map).is_none());
        let mut pinned = Vec::new();
        descriptor.read_to_end(&mut pinned).unwrap();
        assert_eq!(pinned, b"captured file");
        let mut wrong = map.clone();
        wrong.inode = 0;
        assert!(read(pid, &wrong).is_none());
        let large = root.join("large");
        fs::File::create(&large)
            .unwrap()
            .set_len(MAX_FILE_SIZE + 1)
            .unwrap();
        let meta = fs::metadata(&large).unwrap();
        let large_map = crate::privileged_helper::process::maps::parse_map(&format!(
            "1000-2000 r-xp 0 {:x}:{:x} {} {}",
            libc::major(meta.dev()),
            libc::minor(meta.dev()),
            meta.ino(),
            large.display()
        ))
        .unwrap();
        assert!(read(pid, &large_map).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
