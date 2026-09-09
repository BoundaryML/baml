trait RunnerHost {
    fn run<Out>(&self, value: Out) -> Out;
}
fn accept(_: &dyn RunnerHost) {}
fn main() {}
