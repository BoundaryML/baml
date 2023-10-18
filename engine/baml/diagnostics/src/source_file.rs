use std::{path::PathBuf, sync::Arc};

/// A Prisma schema document.
#[derive(Debug, Clone)]
pub struct SourceFile {
    path: PathBuf,
    contents: Contents,
}

impl PartialEq for SourceFile {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for SourceFile {}

impl SourceFile {
    pub fn new_static(path: PathBuf, content: &'static str) -> Self {
        Self {
            path,
            contents: Contents::Static(content),
        }
    }

    pub fn new_allocated(path: PathBuf, s: Arc<str>) -> Self {
        Self {
            path,
            contents: Contents::Allocated(s),
        }
    }

    pub fn as_str(&self) -> &str {
        match self.contents {
            Contents::Static(s) => s,
            Contents::Allocated(ref s) => s,
        }
    }

    pub fn path(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

impl From<(PathBuf, &str)> for SourceFile {
    fn from((path, s): (PathBuf, &str)) -> Self {
        Self::new_allocated(path.clone(), Arc::from(s.to_owned().into_boxed_str()))
    }
}

impl From<(&PathBuf, &String)> for SourceFile {
    fn from((path, s): (&PathBuf, &String)) -> Self {
        Self::new_allocated(path.clone(), Arc::from(s.to_owned().into_boxed_str()))
    }
}

impl From<(PathBuf, Box<str>)> for SourceFile {
    fn from((path, s): (PathBuf, Box<str>)) -> Self {
        Self::new_allocated(path, Arc::from(s))
    }
}

impl From<(PathBuf, Arc<str>)> for SourceFile {
    fn from((path, s): (PathBuf, Arc<str>)) -> Self {
        Self::new_allocated(path, s)
    }
}

impl From<(PathBuf, String)> for SourceFile {
    fn from((path, s): (PathBuf, String)) -> Self {
        Self::new_allocated(path, Arc::from(s.into_boxed_str()))
    }
}

#[derive(Debug, Clone)]
enum Contents {
    Static(&'static str),
    Allocated(Arc<str>),
}
