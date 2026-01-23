//! Resource types for external operations.
//!
//! This crate defines resource types (file handles, sockets) that can be stored
//! on the VM heap. It is separate from `bex_sys` to avoid circular dependencies.

use std::sync::Arc;

use tokio::{fs::File, net::TcpStream, sync::Mutex};

// ============================================================================
// Resource Types
// ============================================================================

/// A file handle stored on the VM heap.
pub struct FileHandle {
    /// The file, wrapped in Arc for cloning.
    pub file: Arc<Mutex<File>>,
    /// The path the file was opened from.
    pub path: String,
}

impl FileHandle {
    /// Create a new file handle.
    pub fn new(file: File, path: String) -> Self {
        Self {
            file: Arc::new(Mutex::new(file)),
            path,
        }
    }
}

impl PartialEq for FileHandle {
    fn eq(&self, other: &Self) -> bool {
        // Compare by Arc pointer equality - same file = same object
        Arc::ptr_eq(&self.file, &other.file)
    }
}

/// A socket handle stored on the VM heap.
pub struct SocketHandle {
    /// The stream, wrapped in Arc for cloning.
    pub stream: Arc<Mutex<TcpStream>>,
    /// The address the socket connected to.
    pub addr: String,
}

impl SocketHandle {
    /// Create a new socket handle.
    pub fn new(stream: TcpStream, addr: String) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            addr,
        }
    }
}

impl PartialEq for SocketHandle {
    fn eq(&self, other: &Self) -> bool {
        // Compare by Arc pointer equality - same socket = same object
        Arc::ptr_eq(&self.stream, &other.stream)
    }
}

// ============================================================================
// Resource Enum
// ============================================================================

/// All resource types that can be stored on the VM heap.
pub enum ResourceKind {
    File(FileHandle),
    Socket(SocketHandle),
}

impl From<FileHandle> for ResourceKind {
    fn from(handle: FileHandle) -> Self {
        ResourceKind::File(handle)
    }
}

impl From<SocketHandle> for ResourceKind {
    fn from(handle: SocketHandle) -> Self {
        ResourceKind::Socket(handle)
    }
}

impl PartialEq for ResourceKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ResourceKind::File(a), ResourceKind::File(b)) => a == b,
            (ResourceKind::Socket(a), ResourceKind::Socket(b)) => a == b,
            _ => false,
        }
    }
}

impl std::fmt::Debug for ResourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceKind::File(h) => write!(f, "File({})", h.path),
            ResourceKind::Socket(h) => write!(f, "Socket({})", h.addr),
        }
    }
}

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceKind::File(h) => write!(f, "file:{}", h.path),
            ResourceKind::Socket(h) => write!(f, "socket:{}", h.addr),
        }
    }
}
