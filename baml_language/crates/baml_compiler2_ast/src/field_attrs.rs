//! Field attribute names.

/// The canonical set of field attribute names. Public so completion can
/// enumerate exactly what [`is_field_attr`] accepts.
pub const FIELD_ATTR_NAMES: &[&str] = &["alias", "description", "skip"];

/// Check if an attribute name is a field attribute.
pub fn is_field_attr(name: &str) -> bool {
    FIELD_ATTR_NAMES.contains(&name)
}
