// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation

//! # aumos-edge-sync
//!
//! Sync engine for the Aumos edge runtime.
//!
//! Handles pushing local governance state to and pulling updated configuration
//! from a remote governance server.
//!
//! ## Protocol
//!
//! 1. Push all local audit records
//! 2. Push budget deltas
//! 3. Pull latest `EdgeConfig`
//! 4. Resolve conflicts (last-write-wins by default)
//!
//! ## Fire Line
//!
//! No smart sync prioritisation. Push everything; pull everything.
//! No ML, no semantic ordering, no adaptive behaviour.

pub mod conflict;
pub mod queue;
pub mod sync;
pub mod transport;

pub use conflict::{ConflictResolution, ConflictResolver, ConflictWinner, ResolutionStrategy};
pub use queue::{ActionQueue, QueuedAction};
pub use sync::{SyncConfig, SyncEngine, SyncError, SyncReport};
pub use transport::HttpTransport;
