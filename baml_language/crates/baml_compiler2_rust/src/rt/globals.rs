use super::{Gen, no, registry};

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn admit(&self, event: &registry::Admission<'_, '_>) -> Gen<()> {
        if matches!(event, registry::Admission::Initializers(true)) {
            return Err(no("top-level initializers"));
        }
        Ok(())
    }
}
