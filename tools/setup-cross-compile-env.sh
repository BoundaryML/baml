#!/usr/bin/env sh

set -euxo pipefail

cat <<EOF

This script is meant for usage inside a Docker image, with boundaryml/baml
mounted into the image as a volume; it installs necessary dependencies for
cross-compiling BAML.

To start such a Docker image with BAML mounted directly:

  docker run -v ~/baml2:/baml2 -it node bash
  docker run -v ~/baml3:/baml3 -it node:alpine sh

Note that we do _not_ suggest running this script against ~/baml - it will require you
to nuke your node_modules

EOF

# Install curl based on the detected operating system
if [ -f /etc/os-release ]; then
    case "$(grep '^ID=' /etc/os-release | cut -d= -f2)" in
        amzn|amazonlinux)
            yum install -y curl
            ;;
        alpine)
            apk add --no-cache curl bash
            ;;
        ubuntu|debian)
            # apt-get update && apt-get install -y curl
            apt install -y curl
            ;;
        *)
            echo "Unsupported operating system. Please install curl manually."
            exit 1
            ;;
    esac
else
    echo "Unable to detect operating system. Please install curl manually."
    exit 1
fi

# set up mise
curl https://mise.run | sh
~/.local/bin/mise install -y pnpm

# set up rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# set up bashrc startup
cat >~/.bashrc <<'EOF'
eval "$(~/.local/bin/mise activate bash)"
. ~/.cargo/env
export PS1="[mise][rust] $PS1"
EOF

echo "Starting bash shell - run 'exit' or press Ctrl-D to close session"

bash