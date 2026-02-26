// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation
//
// basic_edge.rs — minimal end-to-end usage of aumos-edge-core.
//
// Run with:
//   cargo run --example basic_edge

use aumos_edge_core::{
    ActionKind, EdgeConfig, EdgeGovernanceEngine, GovernanceAction, GovernanceOutcome,
    InMemoryStorage,
};

fn main() {
    env_logger::init();

    // ── Build a config in-process (no file needed for this example) ──────────
    let config = EdgeConfig::default();

    // ── Construct the engine with in-memory storage ──────────────────────────
    let mut engine =
        EdgeGovernanceEngine::with_storage(config, Box::new(InMemoryStorage::new()));

    println!("aumos-edge-runtime — basic example");
    println!("====================================");

    // ── Case 1: Standard agent reading a public resource ────────────────────
    let action_read = GovernanceAction::new(
        "agent-001",
        ActionKind::DataRead,
        "public-metrics",
        0.05,
    );
    let decision = engine.evaluate(&action_read);
    println!(
        "[{}] {} on '{}' → {:?} ({})",
        action_read.agent_id,
        format!("{:?}", action_read.kind),
        action_read.resource,
        decision.outcome,
        decision.reason,
    );
    assert_eq!(decision.outcome, GovernanceOutcome::Allowed);

    // ── Case 2: Standard agent attempting a config change ───────────────────
    // Default config has no elevated trust requirement for config_change, so
    // this passes. In a real deployment, set action_requirements in the TOML.
    let action_config = GovernanceAction::new(
        "agent-002",
        ActionKind::ConfigChange,
        "system-settings",
        0.0,
    );
    let decision2 = engine.evaluate(&action_config);
    println!(
        "[{}] {:?} on '{}' → {:?} ({})",
        action_config.agent_id,
        action_config.kind,
        action_config.resource,
        decision2.outcome,
        decision2.reason,
    );

    // ── Case 3: Exhaust budget ───────────────────────────────────────────────
    // Default budget is 50.0 units. Submit actions that consume it.
    println!("\nExhausting budget for agent-003 …");
    for i in 0..6 {
        let action = GovernanceAction::new(
            "agent-003",
            ActionKind::ExternalCall,
            "remote-api",
            10.0, // 6 × 10.0 = 60.0, exceeds default 50.0 after the 5th
        );
        let decision = engine.evaluate(&action);
        println!(
            "  call {} → {:?}",
            i + 1,
            decision.outcome
        );
    }

    println!("\nDone.");
}
