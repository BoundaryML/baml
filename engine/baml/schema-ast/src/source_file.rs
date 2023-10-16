use std::sync::Arc;

/// A Prisma schema document.
#[derive(Debug, Clone)]
pub struct SourceFile {
    contents: Contents,
}

impl SourceFile {
    pub fn new_static(content: &'static str) -> Self {
        Self {
            contents: Contents::Static(content),
        }
    }

    pub fn new_allocated(s: Arc<str>) -> Self {
        Self {
            contents: Contents::Allocated(s),
        }
    }

    pub fn as_str(&self) -> &str {
        match self.contents {
            Contents::Static(s) => s,
            Contents::Allocated(ref s) => s,
        }
    }
}

impl From<&str> for SourceFile {
    fn from(s: &str) -> Self {
        Self::new_allocated(Arc::from(s.to_owned().into_boxed_str()))
    }
}

impl From<&String> for SourceFile {
    fn from(s: &String) -> Self {
        Self::new_allocated(Arc::from(s.to_owned().into_boxed_str()))
    }
}

impl From<Box<str>> for SourceFile {
    fn from(s: Box<str>) -> Self {
        Self::new_allocated(Arc::from(s))
    }
}

impl From<Arc<str>> for SourceFile {
    fn from(s: Arc<str>) -> Self {
        Self::new_allocated(s)
    }
}

impl From<String> for SourceFile {
    fn from(s: String) -> Self {
        Self::new_allocated(Arc::from(s.into_boxed_str()))
    }
}

#[derive(Debug, Clone)]
enum Contents {
    Static(&'static str),
    Allocated(Arc<str>),
}
