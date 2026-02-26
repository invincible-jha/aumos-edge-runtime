// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! # aumos-edge-core
//!
//! On-device governance enforcement for AI agents operating offline or on
//! constrained hardware.
//!
//! ## Overview
//!
//! The crate exposes a single entry point — [`EdgeGovernanceEngine`] — which
//! composes four modules into a sequential evaluation pipeline:
//!
//! 1. **Trust check** ([`trust`]) — static level comparison from config
//! 2. **Budget check** ([`budget`]) — rolling-window cost enforcement
//! 3. **Consent check** ([`consent`]) — grant/revoke records
//! 4. **Audit log** ([`audit`]) — append-only SHA-256 hash chain
//!
//! ## Fire Line
//!
//! This crate:
//! - Does not import from PWM, MAE, STP, or cognitive-loop modules
//! - Does not run on-device LLM inference
//! - Does not implement adaptive behavior
//! - Stores only governance state (trust levels, budgets, consent, audit)
//!
//! ## Example
//!
//! ```rust,no_run
//! use aumos_edge_core::{EdgeGovernanceEngine, GovernanceAction, ActionKind};
//! use std::path::Path;
//!
//! let mut engine = EdgeGovernanceEngine::from_config(Path::new("edge-config.toml"))
//!     .expect("failed to load config");
//!
//! let action = GovernanceAction::new(
//!     "agent-001",
//!     ActionKind::DataRead,
//!     "sensor-readings",
//!     0.1,
//! );
//!
//! let decision = engine.evaluate(&action);
//! println!("Outcome: {:?}", decision.outcome);
//! ```

pub mod audit;
pub mod budget;
pub mod config;
pub mod consent;
pub mod governance;
pub mod storage;
pub mod trust;
pub mod types;

// Re-export the most commonly used types at the crate root for ergonomics.
pub use governance::EdgeGovernanceEngine;
pub use types::{
    ActionKind, CompletedAction, DecisionStage, EdgeError, GovernanceAction, GovernanceDecision,
    GovernanceOutcome, TrustLevel,
};
pub use config::EdgeConfig;
pub use audit::{AuditEntry, AuditFilter};
pub use consent::ConsentRecord;
pub use storage::{InMemoryStorage, Storage, StorageRecord};
