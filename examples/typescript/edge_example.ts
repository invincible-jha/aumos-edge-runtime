// SPDX-License-Identifier: BSL-1.1
// Copyright (c) 2026 MuVeraAI Corporation
//
// edge_example.ts — TypeScript usage of the Aumos edge governance runtime via NAPI-RS.
//
// Demonstrates the full on-device governance workflow from TypeScript:
//
// 1. Build the native addon (one-time setup).
// 2. Load an EdgeConfig from a TOML file.
// 3. Evaluate agent actions — each returns a parsed GovernanceDecision object.
// 4. Run a sync cycle to push local state and pull updated config.
//
// Prerequisites:
//
//   npm install -g @napi-rs/cli
//   cd crates/aumos-edge-node
//   napi build --platform --release --features nodejs
//   npm install
//
// Then from the repo root:
//
//   npx ts-node examples/typescript/edge_example.ts
//
// Or compile first:
//
//   npx tsc --outDir dist examples/typescript/edge_example.ts
//   node dist/edge_example.js

import * as fs from "fs";
import * as os from "os";
import * as path from "path";

// ---------------------------------------------------------------------------
// Import the native addon.
//
// The @aumos/edge-runtime package wraps the NAPI-RS .node addon built from
// crates/aumos-edge-node.  If the addon is not present, a descriptive error
// is thrown at import time.
// ---------------------------------------------------------------------------

import { GovernanceEngine, SyncEngine } from "@aumos/edge-runtime";

// ---------------------------------------------------------------------------
// Domain types — mirror the Rust GovernanceDecision struct.
// These are returned as parsed JSON from engine.evaluate().
// ---------------------------------------------------------------------------

type GovernanceOutcome = "allowed" | "denied" | "requires_consent";
type DecisionStage = "trust_check" | "budget_check" | "consent_check";

interface GovernanceDecision {
  action_id: string;
  outcome: GovernanceOutcome;
  decided_by: DecisionStage;
  reason: string;
  decided_at: string;
}

interface SyncReport {
  started_at: string;
  completed_at: string;
  audit_entries_pushed: number;
  budget_records_pushed: number;
  config_pulled: boolean;
  conflicts_resolved: number;
  warnings: string[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Write a minimal TOML config to a temp file and return its path. */
function writeTempConfig(): string {
  const toml = `
[governance]
default_trust_level = "standard"
deny_unknown_agents = false
default_budget_units = 50.0
default_budget_window_seconds = 3600

[[agents]]
agent_id = "agent-001"
level = "elevated"

[[agents]]
agent_id = "agent-002"
level = "restricted"

[[budgets]]
agent_id = "agent-001"
total_units = 100.0
window_seconds = 3600

[[action_requirements]]
kind = "data_delete"
minimum_level = "elevated"

[[action_requirements]]
kind = "config_change"
minimum_level = "elevated"

[[consent_requirements]]
resource_pattern = "user-records"
required_for_kinds = ["data_write", "data_delete"]
`.trimStart();

  const configPath = path.join(os.tmpdir(), `aumos-edge-example-${Date.now()}.toml`);
  fs.writeFileSync(configPath, toml, "utf8");
  return configPath;
}

/** Parse the JSON string from engine.evaluate() into a GovernanceDecision. */
function parseDecision(raw: string): GovernanceDecision {
  return JSON.parse(raw) as GovernanceDecision;
}

/** Parse the JSON string from syncEngine.sync() into a SyncReport. */
function parseSyncReport(raw: string): SyncReport {
  return JSON.parse(raw) as SyncReport;
}

// ---------------------------------------------------------------------------
// Main example
// ---------------------------------------------------------------------------

function main(): void {
  const configPath = writeTempConfig();
  console.log("aumos-edge-runtime — TypeScript example");
  console.log("=".repeat(42));

  try {
    // ── 1. Load engine from config ──────────────────────────────────────────
    const engine = GovernanceEngine.fromConfig(configPath);
    console.log(`Engine loaded from: ${configPath}\n`);

    // ── 2. Evaluate: allowed read by elevated agent ─────────────────────────
    const decision1 = parseDecision(
      engine.evaluate("agent-001", "data_read", "metrics", 0.5)
    );
    console.log("[Case 1] agent-001 reads 'metrics'");
    console.log(`  outcome  : ${decision1.outcome}`);
    console.log(`  reason   : ${decision1.reason}`);
    console.log();

    // ── 3. Evaluate: denied — restricted agent cannot write ─────────────────
    const decision2 = parseDecision(
      engine.evaluate("agent-002", "data_write", "config-store", 1.0)
    );
    console.log("[Case 2] agent-002 writes 'config-store' (restricted level)");
    console.log(`  outcome  : ${decision2.outcome}`);
    console.log(`  reason   : ${decision2.reason}`);
    console.log();

    // ── 4. Evaluate: denied — action kind requires elevated trust ───────────
    const decision3 = parseDecision(
      engine.evaluate("agent-002", "data_delete", "archive", 2.0)
    );
    console.log("[Case 3] agent-002 deletes from 'archive' (requires elevated)");
    console.log(`  outcome  : ${decision3.outcome}`);
    console.log(`  reason   : ${decision3.reason}`);
    console.log();

    // ── 5. Evaluate: requires_consent for protected resource ────────────────
    const decision4 = parseDecision(
      engine.evaluate("agent-001", "data_write", "user-records", 1.0)
    );
    console.log("[Case 4] agent-001 writes 'user-records' (consent required)");
    console.log(`  outcome  : ${decision4.outcome}`);
    console.log(`  reason   : ${decision4.reason}`);
    console.log();

    // ── 6. Budget exhaustion ────────────────────────────────────────────────
    // agent-001 has a 100-unit budget. Each external call costs 30 units.
    // After four calls the budget is exceeded.
    console.log("Exhausting agent-001 budget with external_call (30 units each):");
    for (let callNumber = 1; callNumber <= 4; callNumber++) {
      const callDecision = parseDecision(
        engine.evaluate("agent-001", "external_call", "payment-api", 30.0)
      );
      console.log(
        `  call ${callNumber}: outcome=${callDecision.outcome}  ` +
          `reason=${JSON.stringify(callDecision.reason)}`
      );
    }
    console.log();

    // ── 7. Custom action kind ───────────────────────────────────────────────
    // Any string not in the ActionKind enum is treated as a Custom variant.
    const decision5 = parseDecision(
      engine.evaluate("agent-001", "summarise_report", "quarterly-data", 0.1)
    );
    console.log("[Case 5] agent-001 custom action 'summarise_report'");
    console.log(`  outcome  : ${decision5.outcome}`);
    console.log(`  decided_by : ${decision5.decided_by}`);
    console.log();

  } finally {
    fs.unlinkSync(configPath);
  }

  // ── 8. Sync engine usage ──────────────────────────────────────────────────
  // SyncEngine pushes local audit records and pulls updated config from the
  // remote server.  In offline deployments the sync call is queued and
  // retried when connectivity returns.
  console.log("Sync engine (connect to a local stub — expected to fail in example):");
  const syncEngine = new SyncEngine(
    "rpi-edge-001",                 // device_id
    "<replace-with-real-token>",    // auth_token
  );

  try {
    const rawReport = syncEngine.sync("http://localhost:9999/sync");
    const report = parseSyncReport(rawReport);
    console.log(`  audit_entries_pushed : ${report.audit_entries_pushed}`);
    console.log(`  config_pulled        : ${report.config_pulled}`);
    console.log(`  warnings             : ${JSON.stringify(report.warnings)}`);
  } catch (error: unknown) {
    // Connection refused is expected here — the example server is not running.
    const message = error instanceof Error ? error.message : String(error);
    console.log(`  sync failed (expected in example): ${message}`);
  }

  console.log("\nDone.");
}

main();
