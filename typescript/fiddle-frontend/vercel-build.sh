set -x
set -e
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
export PATH="/vercel/.cargo/bin:$PATH"

source $HOME/.cargo/env

which cargo

cd ../../engine/baml-schema-wasm
# cargo install
rustup target add wasm32-unknown-unknown
cargo update -p wasm-bindgen
cargo install -f wasm-bindgen-cli@0.2.87

# cargo build
cd ../../typescript/fiddle-frontend

npm add -g tsup

echo "Path: $(pwd)"

turbo build --filter=@baml/prompt-fiddle-next
echo "Path2: $(pwd)"

ls -l
ls -l /vercel/output