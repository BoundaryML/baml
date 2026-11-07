use super::{Gen, no, registry};

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn admit(&self, event: &registry::Admission<'_, '_>) -> Gen<()> {
        match event {
            registry::Admission::ShieldedBlock(true) => Err(no("defer/shielding")),
            registry::Admission::Class { cleanup: true, .. } => Err(no("generic class or cleanup")),
            _ => Ok(()),
        }
    }
}
