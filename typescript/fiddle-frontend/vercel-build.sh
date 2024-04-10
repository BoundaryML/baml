set -x
set -e
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
export PATH="/vercel/.cargo/bin:$PATH"

source $HOME/.cargo/env

which cargo

npm add -g tsup

turbo build --filter=@baml/prompt-fiddle-next