# BAML Install Guide

## Package Managers

Homebrew and AUR install the `baml` wrapper only. They do not install, select, or update a language toolchain during package install or upgrade.

```sh
brew install boundaryml/tap/baml
baml toolchain use canary
```

Nightly users opt in explicitly:

```sh
baml toolchain use nightly
baml toolchain update
```

Package-manager wrapper upgrades use the package manager:

```sh
brew upgrade baml
```

AUR users upgrade with their normal AUR helper, for example `paru -Syu baml-bin`.

## Curl Installer

The curl installer is user-scoped. It installs or updates `~/.baml/bin/baml` and, unless `--wrapper-only` is used, bootstraps the requested toolchain through the wrapper.

```sh
curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s
curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s -- --channel nightly
curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s -- --version 0.11.0 --no-modify-path
```

Docker/CI:

```dockerfile
RUN curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s -- --version 0.11.0 --no-modify-path
ENV PATH="/root/.baml/bin:${PATH}"
```

The Unix installer writes `~/.baml/env`:

```sh
export BAML_HOME="${BAML_HOME:-$HOME/.baml}"
case ":$PATH:" in
  *":$BAML_HOME/bin:"*) ;;
  *) export PATH="$BAML_HOME/bin:$PATH" ;;
esac
```

## Toolchains

Use selects the default and installs if missing:

```sh
baml toolchain use canary
baml toolchain use nightly
baml toolchain use 0.11.0
```

Install downloads without changing the default:

```sh
baml toolchain install 0.11.0
```

Update advances a channel selector only:

```sh
baml toolchain update
```

Check remote freshness without installing or changing selection:

```sh
baml toolchain status
```

List installed toolchains without checking remote metadata:

```sh
baml toolchain list
```

## IDE

IDE installation is explicit and owned by the selected toolchain:

```sh
baml ide install --cursor
baml ide install --code
```

`baml self-update` updates curl-installed wrappers only. Package-manager wrappers refuse self-update and point back to the package manager.
