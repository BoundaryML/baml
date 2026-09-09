// Mirrors the relevant current BamlValue supertrait shape, not its wire codec.
trait BamlValue: Sized {
    fn decode() -> Self;
}
trait ClientInput: BamlValue {
    fn receiver(&self) -> u64;
}

fn accept(_: &dyn ClientInput) {}
fn main() {}
