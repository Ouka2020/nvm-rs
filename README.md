# nvm-rs

A tool to manage Node.js versions on Windows.

## Requirements

- Windows
- Rust toolchain (edition 2024)

## Installation

### From source

```sh
cargo install nvm-windows
```

### From release

Download the latest binary from [GitHub Releases](https://github.com/Ouka2020/nvm-rs/releases).

## Quick start

```sh
# Set the root directory where Node.js versions will be stored
nvm root D:\nodejs

# Set the symlink path (add to your environment variables)
#   NVM_SYMLINK=C:\Program Files\nodejs

# Install the latest version
nvm install latest

# Install a specific version
nvm install 22.5.0

# Switch to a version
nvm use 22.5.0

# List installed versions
nvm ls

# List available remote versions
nvm ls --available

# Uninstall a version
nvm uninstall 22.5.0
```

## Commands

| Command | Alias | Description |
|---|---|---|
| `nvm install <version> [arch] [--insecure]` | `i` | Install a Node.js version. `version` can be `latest`, `lts`, or a semver. `arch` can be `32` or `64` (defaults to system arch). `--insecure` skips SSL validation. |
| `nvm uninstall <version>` | `un` | Uninstall a specific version. |
| `nvm list [--available]` | `ls` | List installed versions, or remote versions with `--available`. |
| `nvm on` | | Enable Node.js version management (creates junction link to the latest installed version). |
| `nvm off` | | Disable Node.js version management (removes junction link). |
| `nvm root [path]` | | Set or display the root directory. |
| `nvm arch` | | Show the current architecture. |
| `nvm proxy [url]` | | Set a proxy for downloads. Use `none` to remove. |
| `nvm current` | | Display the active Node.js version. |
| `nvm node-mirror [url]` | | Set or display the Node.js download mirror. |
| `nvm npm-mirror [url]` | | Set or display the npm download mirror. |
| `nvm use <version> [arch]` | | Switch to a specific version. |

## Configuration

The configuration file is stored next to the `nvm.exe` binary. The format depends on the enabled feature:

- **YAML** (default): `settings.txt`
- **TOML**: `settings.toml`

### Configuration fields

| Field | Description |
|---|---|
| `root` | Directory where Node.js versions are stored. |
| `proxy` | Proxy URL for downloads. Set to `none` to disable. |
| `node_mirror` | Node.js download mirror. Defaults to `https://nodejs.org/dist`. |
| `npm_mirror` | npm download mirror. Defaults to `https://registry.npmjs.org`. |
| `arch` | Default architecture (`32` or `64`). |
| `originalpath` | Previous symlink target (for restoring). |
| `originalversion` | Previous version (for restoring). |

### Environment variables

| Variable | Description |
|---|---|
| `NVM_SYMLINK` | Path where the Node.js junction link is created (e.g. `C:\Program Files\nodejs`). |

## Features

The crate uses Cargo feature flags to customize the build:

| Feature | Default | Description |
|---|---|---|
| `default` | yes | Enables `ureq` + `yaml`. |
| `ureq` | (part of default) | Uses `ureq` as the HTTP client. |
| `yaml` | (part of default) | Uses YAML-format config (`settings.txt`). |
| `toml` | no | Uses TOML-format config (`settings.toml`). |
| `debug` | no | Enables `tracing` logging to console and rotating log files. |

### Build examples

```sh
# Default build
cargo build --release

# With TOML config + debug logging
cargo build --release --no-default-features --features toml,debug

# Debug logging only
cargo build --release --features debug
```

## Acknowledgements

This project is inspired by and based on [nvm-windows](https://github.com/nvm-windows/nvm). Many thanks to the original authors and contributors for their foundational work.

## License

MIT
