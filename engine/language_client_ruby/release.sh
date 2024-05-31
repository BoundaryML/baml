#!/bin/bash
set -x
cd ..
rb-sys-dock --platform x86_64-linux --mount-toolchains --ruby-versions 3.3,3.2,3.1,3.0,2.7 --build --directory language_client_ruby -- sudo yum install -y perl-IPC-Cmd
rb-sys-dock --platform x86_64-darwin --mount-toolchains --ruby-versions 3.3,3.2,3.1,3.0,2.7 --build --directory language_client_ruby
rb-sys-dock --platform arm64-darwin --mount-toolchains --ruby-versions 3.3,3.2,3.1,3.0,2.7 --build --directory language_client_ruby