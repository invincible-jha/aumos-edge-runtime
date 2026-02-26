# Deployment Guide

This guide covers deploying `aumos-edge-runtime` on constrained hardware:
Raspberry Pi single-board computers, mobile devices (Android/iOS via FFI), and
other resource-limited embedded platforms.

---

## Hardware requirements

The runtime has no hard minimum, but the following are practical lower bounds
based on the in-memory storage backend:

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | ARMv7 32-bit | ARM64 (aarch64) |
| RAM | 32 MB available | 128 MB available |
| Storage | 8 MB for binary | 64 MB for binary + audit log |
| OS | Linux (musl or glibc) | Linux, macOS, Android (via JNI) |

The Rust binary compiles to a single self-contained executable with no runtime
dependencies other than libc.

---

## Raspberry Pi (Linux aarch64)

### Cross-compile from a development machine

```bash
# Install the cross-compilation toolchain
rustup target add aarch64-unknown-linux-gnu
cargo install cross

# Build the core library in release mode
cross build --release --target aarch64-unknown-linux-gnu -p aumos-edge-core

# Build the Python bindings (requires maturin and a cross target)
pip install maturin
maturin build --release --target aarch64-unknown-linux-gnu \
    --manifest-path crates/aumos-edge-python/Cargo.toml \
    --features python
```

The compiled `.so` wheel is in `target/wheels/`. Copy it to the Pi and install:

```bash
scp target/wheels/aumos_edge-*.whl pi@raspberrypi:~
ssh pi@raspberrypi
pip install aumos_edge-*.whl
```

### Provisioning the config file

Place `edge-config.toml` in a predictable location before running your agent:

```bash
scp edge-config.toml pi@raspberrypi:/etc/aumos/edge-config.toml
```

Set restrictive file permissions — the config file contains trust assignments:

```bash
chmod 640 /etc/aumos/edge-config.toml
chown root:aumos /etc/aumos/edge-config.toml
```

### Running as a systemd service

Create `/etc/systemd/system/aumos-agent.service`:

```ini
[Unit]
Description=AumOS Edge Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=aumos
Group=aumos
WorkingDirectory=/opt/aumos
ExecStart=/opt/aumos/venv/bin/python /opt/aumos/agent.py
Environment=AUMOS_CONFIG=/etc/aumos/edge-config.toml
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
systemctl enable aumos-agent
systemctl start aumos-agent
journalctl -u aumos-agent -f
```

---

## Mobile (Android via JNI / iOS via Swift FFI)

The `aumos-edge-core` crate can be compiled for Android (AAR) and iOS (XCFramework)
using the standard Rust mobile toolchains. The NAPI-RS and PyO3 bindings are not
used for mobile — call the Rust API directly via JNI (Android) or a C FFI shim
(iOS).

### Android

```bash
rustup target add aarch64-linux-android
rustup target add armv7-linux-androideabi

cargo install cargo-ndk

cargo ndk -t arm64-v8a -t armeabi-v7a \
    -o android/app/src/main/jniLibs \
    build --release -p aumos-edge-core
```

Expose a JNI entry point in a Rust shim crate (`aumos-edge-android`) that wraps
`EdgeGovernanceEngine` and marshals `GovernanceAction` / `GovernanceDecision`
through the JNI boundary.

### iOS

```bash
rustup target add aarch64-apple-ios
rustup target add x86_64-apple-ios

cargo build --release --target aarch64-apple-ios -p aumos-edge-core
cargo build --release --target x86_64-apple-ios -p aumos-edge-core

# Create a universal binary
lipo -create \
    target/aarch64-apple-ios/release/libaumos_edge_core.a \
    target/x86_64-apple-ios/release/libaumos_edge_core.a \
    -output libaumos_edge_core_universal.a
```

Use [`cbindgen`](https://github.com/mozilla/cbindgen) to generate C headers for
the Swift bridging layer.

---

## Constrained Linux (musl static binary)

For devices with restricted package management (industrial PLCs, embedded Linux):

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl -p aumos-edge-core

# Binary has zero shared library dependencies
ldd target/x86_64-unknown-linux-musl/release/aumos-edge-core || echo "statically linked"
```

Strip the binary to minimise flash usage:

```bash
strip target/x86_64-unknown-linux-musl/release/aumos-edge-core
ls -lh target/x86_64-unknown-linux-musl/release/aumos-edge-core
```

---

## Config file management

The TOML configuration file is the single control plane for the edge device.
Update it by either:

1. **Sync pull** — `SyncEngine::sync()` downloads the latest config from the
   server and writes it to the engine's in-memory state. Use `reload_config()`
   to persist the update to disk.
2. **Direct provisioning** — push a new TOML file via your deployment pipeline
   (Ansible, Puppet, or `scp`) and restart the agent.

The runtime never modifies its own TOML file. Config changes are operator-driven.

### Minimal production config

```toml
[governance]
default_trust_level = "restricted"
deny_unknown_agents = true
default_budget_units = 20.0
default_budget_window_seconds = 3600

[[agents]]
agent_id = "my-agent-001"
level = "standard"

[[budgets]]
agent_id = "my-agent-001"
total_units = 50.0
window_seconds = 3600

[[action_requirements]]
kind = "data_delete"
minimum_level = "elevated"

[[action_requirements]]
kind = "config_change"
minimum_level = "elevated"
```

---

## Monitoring and logging

The runtime uses the standard Rust `log` crate. Set the `RUST_LOG` environment
variable to control verbosity:

```bash
RUST_LOG=info   # Recommended for production
RUST_LOG=debug  # Verbose — includes every pipeline step
RUST_LOG=warn   # Only warnings and errors
```

Integrate with your existing log aggregator by connecting a compatible `log`
backend (e.g., `env_logger`, `tracing-subscriber`, `syslog`).

The audit log is the authoritative record of governance decisions. Pull it from
local storage or retrieve it via the sync server for compliance reporting.

---

## Resource usage tips

- Use `InMemoryStorage` for ephemeral agents with short session durations.
- Use a file-backed or SQLite storage backend for devices that run continuously
  or need to survive power interruptions.
- Tune `AgentBudgetConfig::window_seconds` to match your agent's expected
  operating cadence — short windows reduce memory pressure from accumulated
  budget records.
- Run the sync cycle on a low-frequency timer (every 5–30 minutes) to avoid
  unnecessary network usage on metered connections.
- Set `max_queue_attempts = 3` on networks with frequent short outages to
  limit queue growth.
