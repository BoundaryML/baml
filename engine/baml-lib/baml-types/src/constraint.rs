
/// The result of running validation on a value with checks.
#[derive(Clone, Debug, PartialEq)]
pub enum ConstraintsResult {
    Success,
    AssertFailure(ConstraintFailure),
    CheckFailures(Vec<ConstraintFailure>),
}

impl ConstraintsResult {
    /// Combine two `ConstraintsResult`s, following the semantics of asserts and
    /// checks. The first assert short-circuits all other results, otherwise
    /// failed checks combine, returning `Success` if neither result has any
    /// failed checks.
    ///
    pub fn combine(self, other: Self) -> Self {
        use ConstraintsResult::*;
        match (&self, &other) {
            (AssertFailure(_), _) => self,
            (_, AssertFailure(_)) => other,
            (CheckFailures(fs1), CheckFailures(fs2)) => {
                let mut fs = fs1.clone();
                fs.extend_from_slice(fs2);
                CheckFailures(fs.to_vec())
            },
            (Success, _) => other,
            (_, Success) => self,
        }
    }
}

/// A single failure of a user-defined @check or @assert.
#[derive(Clone, Debug, PartialEq)]
pub struct ConstraintFailure {
    // /// The context of the field.
    // pub field_context: Vec<String>,
    // /// The class field that failed the check.
    // pub field_name: String,
    /// The user-supplied name for the check that failed.
    pub constraint_name: String,
}
