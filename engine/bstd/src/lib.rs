mod dedent;
mod project_fqn;
mod random_word_id;

pub use dedent::{dedent, DedentedString};
use num::Integer;
pub use project_fqn::ProjectFqn;
pub use random_word_id::random_word_id;

pub fn pluralize<T: Integer + Copy>(
    qty: T,
    singular: impl Into<String>,
    plural: impl Into<String>,
) -> String {
    if qty == T::one() {
        singular.into()
    } else {
        plural.into()
    }
}
