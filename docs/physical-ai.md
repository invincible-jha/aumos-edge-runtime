<!-- SPDX-License-Identifier: BSL-1.1 -->
<!-- Copyright (c) 2026 MuVeraAI Corporation -->

# AumOS Governance for Physical AI

## Overview

Physical AI systems -- robotics, autonomous vehicles, industrial IoT,
and smart infrastructure -- present unique governance challenges. Unlike
software-only AI agents, physical AI systems can cause irreversible
real-world consequences. A misconfigured trust level or a missed budget
cap in a warehouse robot can result in property damage or human injury.

The AumOS edge runtime provides deterministic, auditable governance
enforcement for these safety-critical environments. Built in Rust, the
runtime offers bounded execution time, memory safety without a garbage
collector, and a tamper-evident audit trail that satisfies regulatory
compliance requirements.

## Safety-Critical Trust Levels

Physical AI systems require a stricter trust level taxonomy than
software agents. The edge runtime's four-level trust model maps to
physical safety tiers:

| Trust Level   | Physical AI Mapping                              | Example Actions                      |
|---------------|--------------------------------------------------|--------------------------------------|
| `Restricted`  | Observation only. No physical actuation.          | Read sensors, log telemetry          |
| `Standard`    | Low-risk physical actions with soft limits.       | Move within safe zone, adjust speed  |
| `Elevated`    | High-risk actions requiring verified operator.    | Operate near humans, lift heavy loads|
| `System`      | Emergency and maintenance operations.             | Emergency stop, firmware update      |

### L4/L5 Physical Actuator Governance

For systems that directly control physical actuators (motors, valves,
robotic arms), governance enforcement follows a conservative model:

- **L4 (Elevated) actions** require that the agent has been explicitly
  assigned `Elevated` trust by a human operator. The trust level is
  never elevated automatically based on past behaviour.
- **L5 (System) actions** are reserved for emergency stop and
  maintenance operations. Only system-level agents with `System` trust
  can trigger these. Human confirmation is enforced at the application
  layer, not within the governance engine.

Trust levels for physical actuators are set during commissioning and
reviewed during maintenance windows. They are never modified at runtime
by the governance engine.

## Deterministic Evaluation Guarantees

The Rust runtime provides properties that are essential for physical AI
governance:

- **No garbage collection pauses.** The governance engine never stops
  the world. Every `evaluate()` call completes in bounded time
  determined only by the number of governance rules, not by heap
  pressure or GC scheduling.
- **No runtime panics in library code.** All error paths return
  `Result` types. The engine does not call `unwrap()` on fallible
  operations in the evaluation pipeline.
- **Deterministic memory usage.** The engine allocates only during
  initialisation (loading config, creating storage maps). The
  evaluation path itself performs no heap allocation beyond the
  decision struct and audit entry.
- **Thread safety.** The `Storage` trait requires `Send + Sync`,
  ensuring that the engine can be shared across threads in
  multi-threaded robotics frameworks without data races.

## Edge-to-Cloud Sync for Audit Compliance

Physical AI systems operating in regulated industries (manufacturing,
logistics, healthcare) must maintain complete audit trails for
compliance. The edge runtime's sync architecture supports this:

```text
+------------------+     +------------------+     +------------------+
|  Robot A (Edge)  |     |  Robot B (Edge)  |     |  Robot C (Edge)  |
|  Local Audit Log |     |  Local Audit Log |     |  Local Audit Log |
+--------+---------+     +--------+---------+     +--------+---------+
         |                        |                        |
         v                        v                        v
+----------------------------------------------------------------+
|                   Sync Engine (HTTP Push)                       |
|                   Push audit | Push budget                     |
+----------------------------------------------------------------+
         |
         v
+----------------------------------------------------------------+
|                Remote Governance Server                         |
|  Aggregated audit trail | Centralised config management        |
|  Compliance reporting   | Operator dashboards                  |
+----------------------------------------------------------------+
```

### Audit Chain Integrity

Every governance decision is appended to a local SHA-256 hash-chained
audit log. The hash chain ensures that:

- No audit entry can be modified after being written.
- No audit entry can be removed without breaking the chain.
- The remote server can verify chain integrity upon receiving pushed
  entries.

### Sync Resilience

Physical AI systems frequently lose connectivity (underground mines,
ocean vessels, warehouse RF dead zones). The sync engine:

- Queues all audit entries locally until connectivity is restored.
- Does not block governance decisions on sync availability.
- Applies conflict resolution using deterministic, rule-based
  strategies (most-restrictive-wins by default).

## Example: Governed Warehouse Robot

A warehouse robot that picks, places, and transports items. The
governance engine controls which actions the robot's AI planner can
execute:

```rust,no_run
use aumos_edge_core::{
    EdgeGovernanceEngine, GovernanceAction, ActionKind, EdgeConfig,
};
use std::path::Path;

// Load governance rules from local config.
let mut engine = EdgeGovernanceEngine::from_config(
    Path::new("/etc/aumos/warehouse-robot.toml"),
).expect("governance config must be valid");

// The robot's AI planner proposes to pick up a heavy item near a human.
let action = GovernanceAction::new(
    "warehouse-bot-017",
    ActionKind::ToolExecution,
    "heavy-lift-zone-3",
    5.0,  // estimated cost in budget units
);

let decision = engine.evaluate(&action);

match decision.outcome {
    aumos_edge_core::GovernanceOutcome::Allowed => {
        // Execute the pick operation.
    }
    aumos_edge_core::GovernanceOutcome::Denied => {
        // Abort and report the denial reason to the fleet controller.
        log::warn!("Action denied: {}", decision.reason);
    }
    aumos_edge_core::GovernanceOutcome::RequiresConsent => {
        // Request operator approval via the fleet management system.
        log::info!("Operator consent required: {}", decision.reason);
    }
}
```

### Configuration Example (`warehouse-robot.toml`)

```toml
[governance]
default_trust_level = "restricted"
deny_unknown_agents = true

[[agents]]
agent_id = "warehouse-bot-017"
level = "elevated"

[[budgets]]
agent_id = "warehouse-bot-017"
total_units = 500.0
window_seconds = 3600

[[action_requirements]]
kind = "tool_execution"
minimum_level = "elevated"

[[action_requirements]]
kind = "config_change"
minimum_level = "system"

[[consent_requirements]]
resource_pattern = "heavy-lift-"
required_for_kinds = ["tool_execution"]
```

## Compliance and Regulatory Considerations

- **ISO 26262 (Automotive).** The deterministic evaluation path and
  complete audit trail support functional safety evidence collection.
- **IEC 62443 (Industrial Automation).** Trust level enforcement maps
  to security level (SL) requirements for industrial control systems.
- **FDA 21 CFR Part 11 (Medical Devices).** The SHA-256 hash-chained
  audit log provides tamper-evident electronic records.

The governance engine does not itself certify compliance with any
standard. It provides the enforcement and auditing primitives that
compliance teams can build upon.
