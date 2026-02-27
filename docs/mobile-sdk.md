<!-- SPDX-License-Identifier: BSL-1.1 -->
<!-- Copyright (c) 2026 MuVeraAI Corporation -->

# AumOS Mobile SDK

## Overview

The AumOS edge runtime can be embedded directly into iOS and Android
applications via Rust FFI, bringing on-device governance enforcement to
mobile AI assistants, voice agents, and other client-side AI workloads.

This document describes the architecture, binding generation strategy,
and integration patterns for mobile platforms.

## Architecture

```text
+-----------------------------------+
|  Swift / Kotlin Application       |
|  (UI layer, AI assistant logic)   |
+-----------------------------------+
          |  FFI boundary
          v
+-----------------------------------+
|  Generated Bindings               |
|  (Swift via UniFFI / cbindgen)    |
|  (Kotlin via UniFFI / JNI)        |
+-----------------------------------+
          |  Rust ABI
          v
+-----------------------------------+
|  aumos-edge-core                  |
|  Trust | Budget | Consent | Audit |
+-----------------------------------+
          |
          v
+-----------------------------------+
|  aumos-edge-sync                  |
|  Push audit | Pull config         |
+-----------------------------------+
```

## Binding Generation

### UniFFI (Recommended)

[UniFFI](https://mozilla.github.io/uniffi-rs/) generates Swift and
Kotlin bindings from a single Rust crate with UDL (Universal Definition
Language) interface definitions. This is the recommended approach for
AumOS mobile SDKs because:

- A single UDL file produces both Swift and Kotlin bindings.
- UniFFI handles memory management across the FFI boundary.
- Error types are automatically bridged to Swift `Error` and Kotlin
  `Exception`.

Example UDL interface (simplified):

```
namespace aumos_edge {
    EdgeGovernanceEngine create_engine(string config_toml);
};

interface EdgeGovernanceEngine {
    GovernanceDecision evaluate(GovernanceAction action);
    void reload_config(string config_toml);
};
```

### cbindgen (C-ABI Fallback)

For environments where UniFFI is not suitable, `cbindgen` generates a
C header from `#[no_mangle] extern "C"` functions. Swift and Kotlin
can call these via their respective C interop mechanisms
(`@_silgen_name` in Swift, JNI in Kotlin).

## Swift Integration (iOS)

### Xcode Setup

1. Build the Rust crate as a static library for iOS targets:
   ```bash
   cargo build --target aarch64-apple-ios --release
   cargo build --target aarch64-apple-ios-sim --release
   ```
2. Add the generated `.a` file and Swift bindings to the Xcode project.
3. Import the module in Swift code.

### Usage Example

```swift
import AumosEdge

let config = """
[governance]
default_trust_level = "standard"
deny_unknown_agents = false
"""

let engine = try createEngine(configToml: config)

let action = GovernanceAction(
    agentId: "voice-assistant",
    kind: .toolExecution,
    resource: "send-message",
    estimatedCost: 1.0
)

let decision = engine.evaluate(action: action)
if decision.outcome == .allowed {
    // Proceed with the action.
}
```

## Kotlin Integration (Android)

### Gradle Setup

1. Build the Rust crate as a shared library for Android targets:
   ```bash
   cargo build --target aarch64-linux-android --release
   cargo build --target armv7-linux-androideabi --release
   cargo build --target x86_64-linux-android --release
   ```
2. Place the `.so` files in `src/main/jniLibs/<abi>/`.
3. Add the generated Kotlin bindings to the project.

### Usage Example

```kotlin
import ai.aumos.edge.EdgeGovernanceEngine
import ai.aumos.edge.GovernanceAction
import ai.aumos.edge.ActionKind

val config = """
[governance]
default_trust_level = "standard"
deny_unknown_agents = false
""".trimIndent()

val engine = EdgeGovernanceEngine.create(configToml = config)

val action = GovernanceAction(
    agentId = "voice-assistant",
    kind = ActionKind.TOOL_EXECUTION,
    resource = "send-message",
    estimatedCost = 1.0
)

val decision = engine.evaluate(action)
when (decision.outcome) {
    GovernanceOutcome.ALLOWED -> { /* proceed */ }
    GovernanceOutcome.DENIED -> { /* show explanation */ }
    GovernanceOutcome.REQUIRES_CONSENT -> { /* prompt user */ }
}
```

## Battery and Network Considerations

### Battery

- The governance engine evaluation is pure CPU work with no I/O. A
  single `evaluate()` call completes in microseconds and has negligible
  battery impact.
- Sync operations (`aumos-edge-sync`) involve network I/O. Schedule
  sync cycles during charging or when the device is on Wi-Fi to
  minimise battery drain.
- Avoid polling sync intervals shorter than 5 minutes on mobile.

### Network

- The sync engine uses HTTP POST/GET with JSON payloads. Payload sizes
  are small (typically under 10 KB per sync cycle).
- All governance decisions are made locally. Network failures do not
  block governance enforcement.
- The sync engine handles intermittent connectivity gracefully: failed
  pushes are re-enqueued, and stale config is detected via the policy
  freshness mechanism.

## Example: Governed Voice Assistant on Mobile

A mobile voice assistant that can send messages, read calendar events,
and make purchases on behalf of the user:

- **Trust levels** are assigned per-agent at app provisioning time.
  The voice assistant starts at `Standard` trust and can be elevated
  to `Elevated` by the user through the settings UI.
- **Budget limits** cap the number of API calls and purchase amounts
  per day. Budgets are static and configured by the app developer.
- **Consent grants** are recorded when the user explicitly approves
  an action category (e.g., "allow this assistant to send messages").
  Consent can be revoked at any time through the settings UI.
- **Audit trail** is stored locally and synced to the server when
  connectivity is available. Users can view their local audit trail
  in the app.
