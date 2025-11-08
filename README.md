# ssht

## Overview

`ssht` is a small CLI for managing SSH local port forwards grouped into named namespaces. Each namespace holds connection parameters and the list of port pairs that should be forwarded from the local machine to a remote target over SSH.

## Building

```bash
cargo build --release
```

The binary is produced at `target/release/ssht` (or `ssht.exe` on Windows).

## Configuration

The application reads its configuration from `SSHT_CONFIG` environment variable if it is set. Otherwise, it falls back to `{HOME_PATH}/.config/ssht/config.toml` on macOS and Linux or `{HOME_PATH}\.config\.ssh\ssht\config.toml` on Windows.

Each namespace entry contains SSH credentials, the remote target IP, and a list of port forwards.

```
[namespaces.office]
ssh_user = "root"
ssh_host = "10.0.0.151"
ssh_port = 22
target_ip = "10.0.0.151"
forwards = [
    { local = 6443, remote = 6443 },
    { local = 80, remote = 80 }
]
```

## Usage

Launch one namespace:

```bash
ssht office
```

Launch several namespaces in one session:

```bash
ssht office qa
```

Each tunnel remains active until the process is stopped (Ctrl+C). For privileged ports (below 1024) run the program with elevated permissions or forward to a higher local port and redirect separately.

