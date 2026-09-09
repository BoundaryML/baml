trait ClientInput {}
struct Client;
impl ClientInput for Client {}
struct Options<'a>(&'a dyn ClientInput);

fn main() {
    let options = {
        let client = Client;
        Options(&client)
    };
    let _ = options.0;
}
