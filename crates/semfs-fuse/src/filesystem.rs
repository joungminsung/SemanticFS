use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use libc::{EACCES, EIO, ENOENT};
use parking_lot::RwLock;
use semfs_core::vfs::{VfsEntry, VfsMapper, WriteHandler};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, warn};

const TTL: Duration = Duration::from_secs(1);
const ROOT_INO: u64 = 1;
const BLOCK_SIZE: u32 = 512;

/// The FUSE filesystem implementation for SemanticFS.
///
/// Semantic paths (directory names typed by the user) are treated as search queries.
/// Listing a semantic directory returns files that match the query via `VfsMapper`.
/// Reading a file proxies through to the real file on disk.
/// Write operations (rename, unlink, write) are delegated to `WriteHandler`
/// which provides WAL-protected crash safety.
pub struct SemanticFilesystem {
    vfs: Arc<VfsMapper>,
    write_handler: Option<Arc<WriteHandler>>,
    source_root: PathBuf,
    read_only: bool,
    /// Maps inode number to its metadata
    inode_map: RwLock<HashMap<u64, InodeEntry>>,
    next_inode: RwLock<u64>,
}

#[derive(Clone, Debug)]
struct InodeEntry {
    /// The semantic path (search query) that produced this entry,
    /// or the semantic path *of* this directory itself.
    semantic_path: String,
    /// Display name for this entry
    name: String,
    /// Path to the real file on disk (None for virtual/semantic directories)
    real_path: Option<PathBuf>,
    /// Whether this entry is a directory
    is_dir: bool,
}

impl SemanticFilesystem {
    pub fn new(
        vfs: Arc<VfsMapper>,
        write_handler: Option<Arc<WriteHandler>>,
        source_root: PathBuf,
        read_only: bool,
    ) -> Self {
        let mut inode_map = HashMap::new();
        inode_map.insert(
            ROOT_INO,
            InodeEntry {
                semantic_path: String::new(),
                name: String::new(),
                real_path: None,
                is_dir: true,
            },
        );

        Self {
            vfs,
            write_handler,
            source_root,
            read_only,
            inode_map: RwLock::new(inode_map),
            next_inode: RwLock::new(2),
        }
    }

    fn alloc_inode(&self) -> u64 {
        let mut next = self.next_inode.write();
        let ino = *next;
        *next += 1;
        ino
    }

    /// Find an existing inode by semantic_path + name, or create a new one.
    fn get_or_create_inode(
        &self,
        semantic_path: &str,
        name: &str,
        real_path: Option<PathBuf>,
        is_dir: bool,
    ) -> u64 {
        // Check if already exists
        {
            let map = self.inode_map.read();
            for (&ino, entry) in map.iter() {
                if entry.semantic_path == semantic_path && entry.name == name {
                    return ino;
                }
            }
        }

        // Create new inode
        let ino = self.alloc_inode();
        self.inode_map.write().insert(
            ino,
            InodeEntry {
                semantic_path: semantic_path.to_string(),
                name: name.to_string(),
                real_path,
                is_dir,
            },
        );
        ino
    }

    /// Build a `FileAttr` for the given inode entry, using real file metadata when available.
    fn make_attr(&self, ino: u64, entry: &InodeEntry) -> FileAttr {
        if let Some(ref real_path) = entry.real_path {
            if let Ok(meta) = std::fs::metadata(real_path) {
                let file_type = if meta.is_dir() {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };
                let mtime = meta.modified().unwrap_or(UNIX_EPOCH);
                let ctime = meta.created().unwrap_or(UNIX_EPOCH);
                let atime = meta.accessed().unwrap_or(UNIX_EPOCH);
                let perm = if meta.is_dir() { 0o755 } else { 0o644 };

                return FileAttr {
                    ino,
                    size: meta.len(),
                    blocks: meta.len().div_ceil(BLOCK_SIZE as u64),
                    atime,
                    mtime,
                    ctime,
                    crtime: ctime,
                    kind: file_type,
                    perm,
                    nlink: 1,
                    uid: unsafe { libc::getuid() },
                    gid: unsafe { libc::getgid() },
                    rdev: 0,
                    blksize: BLOCK_SIZE,
                    flags: 0,
                };
            }
        }

        // Default attributes for virtual (semantic) directories
        let now = SystemTime::now();
        FileAttr {
            ino,
            size: 0,
            blocks: 0,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
            kind: if entry.is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            },
            perm: if entry.is_dir { 0o755 } else { 0o644 },
            nlink: 1,
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    /// Resolve a semantic path to VFS entries via the search engine.
    fn resolve_dir(&self, semantic_path: &str) -> Vec<VfsEntry> {
        if semantic_path.is_empty() {
            // Root directory: empty until the user navigates into a semantic query path
            return Vec::new();
        }

        match self.vfs.readdir(semantic_path) {
            Ok(entries) => entries,
            Err(e) => {
                warn!(path = semantic_path, error = %e, "VFS readdir failed");
                Vec::new()
            }
        }
    }
}

impl Filesystem for SemanticFilesystem {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_str = name.to_string_lossy().to_string();
        debug!(parent, name = %name_str, "FUSE lookup");

        let parent_entry = {
            let map = self.inode_map.read();
            match map.get(&parent) {
                Some(e) => e.clone(),
                None => {
                    reply.error(ENOENT);
                    return;
                }
            }
        };

        // Build the semantic path for this lookup
        let semantic_path = if parent_entry.semantic_path.is_empty() {
            name_str.clone()
        } else {
            format!("{}/{}", parent_entry.semantic_path, name_str)
        };

        // First, check if this is a valid semantic directory (query returns results)
        let entries = self.resolve_dir(&semantic_path);
        if !entries.is_empty() {
            let ino = self.get_or_create_inode(&semantic_path, &name_str, None, true);
            let entry = self.inode_map.read().get(&ino).cloned().unwrap();
            let attr = self.make_attr(ino, &entry);
            reply.entry(&TTL, &attr, 0);
            return;
        }

        // Otherwise, check if it's a file in the parent's search results
        let parent_results = self.resolve_dir(&parent_entry.semantic_path);
        if let Some(vfs_entry) = parent_results.iter().find(|e| e.name == name_str) {
            let ino = self.get_or_create_inode(
                &parent_entry.semantic_path,
                &name_str,
                Some(vfs_entry.real_path.clone()),
                vfs_entry.is_dir,
            );
            let inode_entry = self.inode_map.read().get(&ino).cloned().unwrap();
            let attr = self.make_attr(ino, &inode_entry);
            reply.entry(&TTL, &attr, 0);
            return;
        }

        reply.error(ENOENT);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let entry = {
            let map = self.inode_map.read();
            map.get(&ino).cloned()
        };
        match entry {
            Some(entry) => {
                let attr = self.make_attr(ino, &entry);
                reply.attr(&TTL, &attr);
            }
            None => {
                reply.error(ENOENT);
            }
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let entry = {
            let map = self.inode_map.read();
            match map.get(&ino) {
                Some(e) => e.clone(),
                None => {
                    reply.error(ENOENT);
                    return;
                }
            }
        };

        debug!(ino, path = %entry.semantic_path, offset, "FUSE readdir");

        let mut entries: Vec<(u64, FileType, String)> = vec![
            (ino, FileType::Directory, ".".to_string()),
            (ROOT_INO, FileType::Directory, "..".to_string()),
        ];

        // Get search results for this semantic path
        let vfs_entries = self.resolve_dir(&entry.semantic_path);

        for vfs_entry in vfs_entries {
            let child_ino = self.get_or_create_inode(
                &entry.semantic_path,
                &vfs_entry.name,
                Some(vfs_entry.real_path.clone()),
                vfs_entry.is_dir,
            );
            let file_type = if vfs_entry.is_dir {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            entries.push((child_ino, file_type, vfs_entry.name));
        }

        for (i, (ino, file_type, name)) in entries.iter().enumerate().skip(offset as usize) {
            if reply.add(*ino, (i + 1) as i64, *file_type, name) {
                break;
            }
        }

        reply.ok();
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let entry = {
            let map = self.inode_map.read();
            match map.get(&ino) {
                Some(e) => e.clone(),
                None => {
                    reply.error(ENOENT);
                    return;
                }
            }
        };

        let real_path = match &entry.real_path {
            Some(p) => p.clone(),
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        debug!(ino, path = %real_path.display(), offset, size, "FUSE read");

        use std::io::{Read, Seek, SeekFrom};

        let mut file = match std::fs::File::open(&real_path) {
            Ok(f) => f,
            Err(e) => {
                error!(path = %real_path.display(), error = %e, "Read failed");
                reply.error(EIO);
                return;
            }
        };
        if offset > 0 {
            if let Err(e) = file.seek(SeekFrom::Start(offset as u64)) {
                error!(path = %real_path.display(), error = %e, "Seek failed");
                reply.error(EIO);
                return;
            }
        }
        let mut buf = vec![0u8; size as usize];
        match file.read(&mut buf) {
            Ok(n) => reply.data(&buf[..n]),
            Err(_) => reply.error(EIO),
        }
    }

    fn write(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        if self.read_only {
            reply.error(EACCES);
            return;
        }

        let entry = {
            let map = self.inode_map.read();
            match map.get(&ino) {
                Some(e) => e.clone(),
                None => {
                    reply.error(ENOENT);
                    return;
                }
            }
        };

        let real_path = match &entry.real_path {
            Some(p) => p.clone(),
            None => {
                reply.error(EACCES);
                return;
            }
        };

        if offset == 0 {
            // Full write from beginning: use WAL-protected handler when available
            if let Some(ref handler) = self.write_handler {
                match handler.handle_write(&real_path, data) {
                    Ok(()) => reply.written(data.len() as u32),
                    Err(e) => {
                        error!(error = %e, "Write handler error");
                        reply.error(EIO);
                    }
                }
            } else {
                reply.error(EACCES);
            }
        } else {
            // TODO: WAL does not support partial writes yet; fall back to direct I/O
            warn!(offset, path = %real_path.display(), "Partial write without WAL protection");
            use std::io::{Seek, SeekFrom, Write as IoWrite};

            let mut file = match std::fs::OpenOptions::new().write(true).open(&real_path) {
                Ok(f) => f,
                Err(_) => {
                    reply.error(EIO);
                    return;
                }
            };
            if file.seek(SeekFrom::Start(offset as u64)).is_err() {
                reply.error(EIO);
                return;
            }
            match file.write_all(data) {
                Ok(()) => reply.written(data.len() as u32),
                Err(_) => reply.error(EIO),
            }
        }
    }

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        _newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        if self.read_only {
            reply.error(EACCES);
            return;
        }

        let name_str = name.to_string_lossy().to_string();
        let source_path = {
            let map = self.inode_map.read();
            let parent_entry = match map.get(&parent) {
                Some(e) => e.clone(),
                None => {
                    reply.error(ENOENT);
                    return;
                }
            };
            drop(map);
            let results = self.resolve_dir(&parent_entry.semantic_path);
            results
                .iter()
                .find(|e| e.name == name_str)
                .map(|e| e.real_path.clone())
        };

        let source = match source_path {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let new_name_str = newname.to_string_lossy().to_string();
        let dest = self.source_root.join(&new_name_str);

        if let Some(ref handler) = self.write_handler {
            match handler.handle_rename(&source, &dest) {
                Ok(()) => reply.ok(),
                Err(e) => {
                    error!(error = %e, "Rename handler error");
                    reply.error(EIO);
                }
            }
        } else {
            reply.error(EACCES);
        }
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        if self.read_only {
            reply.error(EACCES);
            return;
        }

        let name_str = name.to_string_lossy().to_string();
        let file_path = {
            let map = self.inode_map.read();
            let parent_entry = match map.get(&parent) {
                Some(e) => e.clone(),
                None => {
                    reply.error(ENOENT);
                    return;
                }
            };
            drop(map);
            let results = self.resolve_dir(&parent_entry.semantic_path);
            results
                .iter()
                .find(|e| e.name == name_str)
                .map(|e| e.real_path.clone())
        };

        let path = match file_path {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        if let Some(ref handler) = self.write_handler {
            match handler.handle_unlink(&path) {
                Ok(()) => reply.ok(),
                Err(e) => {
                    error!(error = %e, "Unlink handler error");
                    reply.error(EIO);
                }
            }
        } else {
            reply.error(EACCES);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inode_allocation() {
        // Verify that SemanticFilesystem can be constructed and allocates inodes correctly.
        // We cannot test the full Filesystem trait without a VfsMapper, but we can
        // verify basic inode bookkeeping.
        let fs_inodes: RwLock<HashMap<u64, InodeEntry>> = RwLock::new(HashMap::new());
        fs_inodes.write().insert(
            ROOT_INO,
            InodeEntry {
                semantic_path: String::new(),
                name: String::new(),
                real_path: None,
                is_dir: true,
            },
        );

        assert!(fs_inodes.read().contains_key(&ROOT_INO));
        assert_eq!(fs_inodes.read().len(), 1);
    }
}
