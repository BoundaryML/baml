trait ClientInput {}
struct Client;
impl ClientInput for Client {}

fn main() {
    let client = Client;
    let concrete: Vec<&Client> = vec![&client];
    let _: Vec<&dyn ClientInput> = concrete;
}
