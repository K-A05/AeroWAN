# AeroWAN

AeroWAN is a peer-to-peer mesh networking daemon that combines [Reticulum](https://reticulum.network/) and [Iroh](https://iroh.computer/) (QUIC-based) transports into a single node, with a terminal user interface for managing connections and chatting with peers.

---

## Core Features

- **Dual-transport architecture** — Reticulum mesh routing runs alongside Iroh QUIC connections in the same daemon process
- **Iroh peer connections** — connect to remote nodes by NodeID with NAT traversal via DERP relay
- **Terminal UI (TUI)** — interactive interface for monitoring node status, managing peers, and chatting
- **Peer-to-peer chat** — real-time text messaging over direct QUIC streams between connected nodes
- **Local REST API** — Bearer-token-authenticated HTTP API that the TUI and other tools can use to control the daemon
- **Persistent node identity** — Ed25519 keypair is generated on first run and reused across restarts so your NodeID stays stable

---

## Dependencies

### System requirements

- **Rust toolchain** — 1.75 or later (install via [rustup](https://rustup.rs))
- **Linux** (kernel 5.4+)

### Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Clone the repository

```bash
git clone https://github.com/K-A05/AeroWAN.git
cd aerowan
```

### Build

```bash
cargo build --release
```

The compiled binary will be at `target/release/aerowan`.

---

## Configuration

On first run, AeroWAN creates a default config file at: `~/.config/aerowan/config.toml`

You can edit this file before running to customise ports and interfaces. The defaults work out of the box for most setups.

Key fields:

```toml
[api]
port = 37430          # port the local REST API listens on

[iroh]
enabled = true        # set false for Reticulum-only mode
bind_port = 0         # 0 = OS assigns a free port

[logging]
loglevel = 4          # 1=error 2=warn 3-4=info 5-6=debug 7+=trace
```

---

## Running

### Headless daemon (no UI)

```bash
./target/release/aerowan
```

The daemon starts, binds the Iroh endpoint, and waits for a shutdown signal. Press `Ctrl-C` to stop.

### Daemon + TUI (recommended)

```bash
./target/release/aerowan --tui
```

This starts the daemon and launches the TUI in the same terminal window. Quitting the TUI (`q`) also shuts down the daemon cleanly.

### TUI only (attach to a running daemon)

```bash
./target/release/aerowan --tui-only
```

Use this if you started the daemon separately and want to attach a UI to it.

---

## Using the TUI

### Main screen

| Key | Action |
|-----|--------|
| `c` | Enter a remote NodeID to connect to a peer |
| `t` | Open the chat peer selector (requires at least one connected peer) |
| `q` | Quit and shut down the daemon |

When connecting, paste the full NodeID of the remote node and press `Enter`. Press `Esc` to cancel.

### Chat screen

Navigate to a peer with `↑` / `↓` and press `Enter` to open a chat session.

| Key | Action |
|-----|--------|
| `i` | Start typing a message |
| `Enter` | Send the message |
| `Esc` | Cancel typing / return to main screen |

Messages you send appear in **cyan**. Messages received from the peer appear in **white**.

---

## Finding your NodeID

Your NodeID is displayed in the TUI header once the daemon has started. You can also query it directly:

```bash
curl -s -H "Authorization: Bearer $(cat ~/.config/aerowan/api.key)" \
  http://127.0.0.1:37430/status
```

Share this NodeID with a peer so they can connect to you.

---

## API Key

An API key is generated automatically on first run and stored at `~/.config/aerowan/api.key`. All REST API requests must include it as a Bearer token:

```
Authorization: Bearer <contents of api.key>
```

---

## REST API Reference

All endpoints are on `http://127.0.0.1:<api.port>` and require the Bearer token.

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/status` | Returns local NodeID and transport mode |
| `GET` | `/peers` | Returns JSON array of connected peer NodeIDs |
| `POST` | `/connect` | Dial a peer — body: `{ "node_id": "..." }` |
| `POST` | `/chat/send` | Send a message — body: `{ "node_id": "...", "message": "..." }` |
| `GET` | `/chat/messages` | Drain and return pending inbound messages |

---

## Known Limitations

- **No log file output** — daemon logs are currently suppressed when launched via `--tui`. To see logs, run the daemon headless in a separate terminal.
- **Reticulum transport not fully integrated** — the Reticulum stack initialises alongside Iroh but mesh routing and Reticulum-native peer discovery are not yet exposed through the TUI or API.
- **No message persistence** — chat history exists in memory only and is lost when the daemon is stopped or the chat screen is closed.
- **Single active chat session** — the TUI supports chatting with one peer at a time; there is no multi-conversation view.
- **No relay configuration UI** — the DERP relay URL must be set manually in `config.toml`; it cannot be changed at runtime.
