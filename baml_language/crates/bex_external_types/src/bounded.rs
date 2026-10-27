//! Host-chosen text as a message carries it.

use std::fmt;

/// A piece of text a host chose (a field name, a key, a value's rendering, a
/// type it stated), cut to what a message shows. It can only be built cut, so
/// no refusal, path or diagnostic echoes a host's value in full.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Bounded(String);

impl Bounded {
    /// How much of a host's text a message shows: a name, a key, a value.
    const TEXT: usize = 80;
    /// How much of a type's rendering a message shows: longer than a name,
    /// as a type carries structure, but never a host-stated union in full.
    const TYPE: usize = 200;

    /// `text` as a message shows it.
    pub fn text(text: impl AsRef<str>) -> Self {
        Self::cut(text.as_ref(), Self::TEXT)
    }

    /// A type's rendering as a message shows it.
    pub fn ty(rendering: impl AsRef<str>) -> Self {
        Self::cut(rendering.as_ref(), Self::TYPE)
    }

    fn cut(text: &str, limit: usize) -> Self {
        match text.char_indices().nth(limit) {
            None => Self(text.to_string()),
            Some((end, _)) => {
                let mut shown = text[..end].to_string();
                shown.push('…');
                Self(shown)
            }
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Bounded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Bounded {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Bounded;

    #[test]
    fn short_text_is_kept_whole() {
        assert_eq!(Bounded::text("name").as_str(), "name");
        assert_eq!(Bounded::ty("int | string").as_str(), "int | string");
    }

    #[test]
    fn long_text_is_cut_with_an_ellipsis_on_a_character_boundary() {
        let long: String = "é".repeat(300);
        let text = Bounded::text(&long);
        assert_eq!(text.as_str().chars().count(), 81);
        assert!(text.as_str().ends_with('…'));
        let ty = Bounded::ty(&long);
        assert_eq!(ty.as_str().chars().count(), 201);
    }
}
