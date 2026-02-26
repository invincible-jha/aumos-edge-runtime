// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! PyO3 Python binding stubs for `aumos-edge-runtime`.
//!
//! This crate exposes `GovernanceEngine` and `SyncEngine` to Python via PyO3.
//!
//! ## Building
//!
//! Requires [maturin](https://github.com/PyO3/maturin) and the `python` feature:
//!
//! ```bash
//! pip install maturin
//! maturin develop --features python
//! ```
//!
//! ## Usage (Python)
//!
//! ```python
//! from aumos_edge import GovernanceEngine, SyncEngine
//!
//! engine = GovernanceEngine.from_config("edge-config.toml")
//! decision = engine.evaluate("agent-001", "data_read", "sensor-data", 0.1)
//! print(decision)
//! ```

#[cfg(feature = "python")]
mod python_bindings {
    use aumos_edge_core::{
        ActionKind, EdgeConfig, EdgeGovernanceEngine, GovernanceAction, InMemoryStorage,
    };
    use aumos_edge_sync::{ActionQueue, SyncConfig, SyncEngine};
    use pyo3::exceptions::PyRuntimeError;
    use pyo3::prelude::*;
    use std::path::Path;

    // ── GovernanceEngine ──────────────────────────────────────────────────────

    /// Python-facing wrapper around [`EdgeGovernanceEngine`].
    ///
    /// All methods return Python-native types (str, dict) to avoid requiring
    /// Rust-side type knowledge in the Python caller.
    #[pyclass(name = "GovernanceEngine")]
    struct PyGovernanceEngine {
        inner: EdgeGovernanceEngine,
    }

    #[pymethods]
    impl PyGovernanceEngine {
        /// Load config from a TOML file at `path`.
        #[staticmethod]
        fn from_config(path: &str) -> PyResult<Self> {
            let engine = EdgeGovernanceEngine::from_config(Path::new(path))
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            Ok(Self { inner: engine })
        }

        /// Evaluate an action and return the decision as a JSON string.
        ///
        /// `kind` must be one of: `data_read`, `data_write`, `data_delete`,
        /// `external_call`, `tool_execution`, `config_change`, or any string
        /// prefixed with `custom:`.
        fn evaluate(
            &mut self,
            agent_id: &str,
            kind: &str,
            resource: &str,
            estimated_cost: f64,
        ) -> PyResult<String> {
            let action_kind = parse_action_kind(kind)?;
            let action = GovernanceAction::new(agent_id, action_kind, resource, estimated_cost);
            let decision = self.inner.evaluate(&action);
            serde_json::to_string(&decision)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))
        }

        /// Record a consent grant for (agent_id, resource, action_kind).
        fn grant_consent(
            &mut self,
            agent_id: &str,
            resource: &str,
            action_kind: &str,
        ) -> PyResult<()> {
            // Consent recording requires mutable storage access; we call through
            // the engine's internal storage via the ConsentStore.
            // This is a stub — full implementation wires through engine internals.
            let _ = (agent_id, resource, action_kind);
            Ok(())
        }
    }

    fn parse_action_kind(kind: &str) -> PyResult<ActionKind> {
        match kind {
            "data_read" => Ok(ActionKind::DataRead),
            "data_write" => Ok(ActionKind::DataWrite),
            "data_delete" => Ok(ActionKind::DataDelete),
            "external_call" => Ok(ActionKind::ExternalCall),
            "tool_execution" => Ok(ActionKind::ToolExecution),
            "config_change" => Ok(ActionKind::ConfigChange),
            other => Ok(ActionKind::Custom(other.to_string())),
        }
    }

    // ── SyncEngine ────────────────────────────────────────────────────────────

    /// Python-facing wrapper around [`SyncEngine`].
    #[pyclass(name = "SyncEngine")]
    struct PySyncEngine {
        inner: SyncEngine,
    }

    #[pymethods]
    impl PySyncEngine {
        /// Create a sync engine with the given device credentials.
        #[new]
        fn new(device_id: &str, auth_token: &str) -> Self {
            let sync_config = SyncConfig {
                device_id: device_id.to_string(),
                auth_token: auth_token.to_string(),
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
        fn sync(&mut self, server_url: &str) -> PyResult<String> {
            let report = self
                .inner
                .sync(server_url)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
            serde_json::to_string(&report)
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))
        }
    }

    // ── Module registration ───────────────────────────────────────────────────

    #[pymodule]
    fn aumos_edge(module: &Bound<'_, PyModule>) -> PyResult<()> {
        module.add_class::<PyGovernanceEngine>()?;
        module.add_class::<PySyncEngine>()?;
        Ok(())
    }
}

// When the `python` feature is not enabled, expose a no-op entry point so the
// workspace compiles cleanly.
#[cfg(not(feature = "python"))]
pub fn _placeholder() {}
