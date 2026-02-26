# Changelog

All notable changes to `aumos-edge-runtime` will be documented here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [Unreleased]

### Added
- `aumos-edge-core`: trust checking, budget enforcement, consent store, append-only audit log with SHA-256 hash chain, and `EdgeGovernanceEngine` composition layer
- `aumos-edge-sync`: `SyncEngine` with push-local / pull-remote semantics, last-write-wins `ConflictResolver`, offline `ActionQueue`, and `HttpTransport`
- `aumos-edge-python`: PyO3 binding stubs exposing `GovernanceEngine` and `SyncEngine` to Python
- `aumos-edge-node`: NAPI-RS binding stubs exposing `GovernanceEngine` and `SyncEngine` to TypeScript / Node.js
- Example programs in Rust, Python, and TypeScript
- Architecture, offline-mode, sync-protocol, and deployment documentation
