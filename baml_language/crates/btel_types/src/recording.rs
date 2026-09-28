/// The UUID scope shared by a recording and its public span identities.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecordingId([u8; 16]);

impl RecordingId {
    pub fn generate() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        (bytes != [0; 16]).then_some(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
