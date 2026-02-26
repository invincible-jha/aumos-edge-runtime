// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! NAPI-RS Node.js/TypeScript binding stubs for `aumos-edge-runtime`.
//!
//! This crate exposes `GovernanceEngine` and `SyncEngine` to TypeScript via
//! NAPI-RS.
//!
//! ## Building
//!
//! Requires [@napi-rs/cli](https://napi.rs/):
//!
//! ```bash
//! npm install -g @napi-rs/cli
//! napi build --platform --release --features nodejs
//! ```
//!
//! ## Usage (TypeScript)
//!
//! ```typescript
//! import { GovernanceEngine, SyncEngine } from "@aumos/edge-runtime";
//!
//! const engine = GovernanceEngine.fromConfig("edge-config.toml");
//! const decision = engine.evaluate("agent-001", "data_read", "sensor-data", 0.1);
//! console.log(JSON.parse(decision));
//! ```

#[cfg(feature = "nodejs")]
mod node_bindings {
    use aumos_edge_core::{
        ActionKind, EdgeGovernanceEngine, GovernanceAction, InMemoryStorage,
    };
    use aumos_edge_sync::{ActionQueue, SyncConfig, SyncEngine};
    use napi::bindgen_prelude::*;
    use napi_derive::napi;
    use std::path::Path;

    // ── GovernanceEngine ──────────────────────────────────────────────────────

    /// Node.js-facing wrapper around [`EdgeGovernanceEngine`].
    ///
    /// All methods accept and return JavaScript-native types (strings, numbers).
    /// Decision and report objects are returned as JSON strings for simplicity.
    #[napi]
    pub struct GovernanceEngine {
        inner: EdgeGovernanceEngine,
    }

    #[napi]
    impl GovernanceEngine {
        /// Load config from a TOML file at `config_path`.
        #[napi(factory)]
        pub fn from_config(config_path: String) -> napi::Result<Self> {
            let engine = EdgeGovernanceEngine::from_config(Path::new(&config_path))
                .map_err(|error| napi::Error::from_reason(error.to_string()))?;
            Ok(Self { inner: engine })
        }

        /// Evaluate an action and return the decision as a JSON string.
        ///
        /// `kind` must be one of: `data_read`, `data_write`, `data_delete`,
        /// `external_call`, `tool_execution`, `config_change`, or any string
        /// treated as `Custom`.
        #[napi]
        pub fn evaluate(
            &mut self,
            agent_id: String,
            kind: String,
            resource: String,
            estimated_cost: f64,
        ) -> napi::Result<String> {
            let action_kind = parse_action_kind(&kind);
            let action = GovernanceAction::new(&agent_id, action_kind, &resource, estimated_cost);
            let decision = self.inner.evaluate(&action);
            serde_json::to_string(&decision)
                .map_err(|error| napi::Error::from_reason(error.to_string()))
        }

        /// Return the number of entries in the audit log.
        #[napi]
        pub fn audit_count(&self) -> napi::Result<u32> {
            // Simplified access — full implementation wires through engine internals.
            Ok(0)
        }
    }

    fn parse_action_kind(kind: &str) -> ActionKind {
        match kind {
            "data_read" => ActionKind::DataRead,
            "data_write" => ActionKind::DataWrite,
            "data_delete" => ActionKind::DataDelete,
            "external_call" => ActionKind::ExternalCall,
            "tool_execution" => ActionKind::ToolExecution,
            "config_change" => ActionKind::ConfigChange,
            other => ActionKind::Custom(other.to_string()),
        }
    }

    // ── SyncEngine ────────────────────────────────────────────────────────────

    /// Node.js-facing wrapper around [`SyncEngine`].
    #[napi]
    pub struct SyncEngineJs {
        inner: SyncEngine,
    }

    #[napi]
    impl SyncEngineJs {
        /// Create a sync engine with the given device credentials.
        #[napi(constructor)]
        pub fn new(device_id: String, auth_token: String) -> Self {
            let sync_config = SyncConfig {
                device_id,
                auth_token,
                conflict_strategy: Default::default(),
                max_queue_attempts: 5,
            };
            let storage = Box::new(InMemoryStorage::new());
            let queue = ActionQueue::new();
            Self {
                inner: SyncEngine::new(sync_config, storage, queue),
            }
        }

        /// Run a full sync cycle against `server_url`.
        ///
        /// Returns the sync report as a JSON string.
        #[napi]
        pub fn sync(&mut self, server_url: String) -> napi::Result<String> {
            let report = self
                .inner
                .sync(&server_url)
                .map_err(|error| napi::Error::from_reason(error.to_string()))?;
            serde_json::to_string(&report)
                .map_err(|error| napi::Error::from_reason(error.to_string()))
        }
    }
}

// When the `nodejs` feature is not enabled, expose a no-op entry point so
// the workspace compiles cleanly.
#[cfg(not(feature = "nodejs"))]
pub fn _placeholder() {}
