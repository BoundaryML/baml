
pub trait Encode {
    type To;

    fn encode(&self) -> Self::To;
}

pub trait Decode {
    type From<'a>;

    fn decode(from: Self::From<'_>) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}
