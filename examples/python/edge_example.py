# SPDX-License-Identifier: BSL-1.1
# Copyright (c) 2026 MuVeraAI Corporation

"""edge_example.py — Python usage of the Aumos edge governance runtime via PyO3.

Demonstrates the full on-device governance workflow:

1. Build the native extension (one-time setup).
2. Load an EdgeConfig from a TOML file.
3. Evaluate agent actions — each returns a structured decision dict.
4. Queue completed actions for the next sync cycle.
5. Run a sync cycle to push local state and pull updated config.

Prerequisites:

    pip install maturin
    cd crates/aumos-edge-python
    maturin develop --features python

Then from the repo root:

    python examples/python/edge_example.py
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Import the native extension via the aumos_edge package shim.
# The shim in crates/aumos-edge-python/python/aumos_edge/__init__.py raises
# a descriptive ImportError if the native .so/.pyd has not been built yet.
# ---------------------------------------------------------------------------

try:
    from aumos_edge import GovernanceEngine, SyncEngine
    import aumos_edge as _aumos_edge
except ImportError as exc:
    print(
        "[ERROR] aumos_edge native extension is not built.\n"
        "Run the following commands first:\n\n"
        "    pip install maturin\n"
        "    cd crates/aumos-edge-python\n"
        "    maturin develop --features python\n",
        file=sys.stderr,
    )
    raise SystemExit(1) from exc


# ---------------------------------------------------------------------------
# Helper: write a minimal TOML config to a temp file so this example is
# self-contained.  In production, this file is provisioned by your deployment
# pipeline.
# ---------------------------------------------------------------------------

_EXAMPLE_TOML = """\
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
"""


def write_temp_config() -> Path:
    """Write the example TOML to a temp file and return its path."""
    config_file = Path(tempfile.mktemp(suffix=".toml"))
    config_file.write_text(_EXAMPLE_TOML, encoding="utf-8")
    return config_file


# ---------------------------------------------------------------------------
# Helper: parse the JSON decision string into a readable dict.
# ---------------------------------------------------------------------------

def parse_decision(raw_json: str) -> dict[str, Any]:
    """Parse the JSON string returned by GovernanceEngine.evaluate."""
    return json.loads(raw_json)


# ---------------------------------------------------------------------------
# Main example
# ---------------------------------------------------------------------------

def main() -> None:
    config_path = write_temp_config()
    print("aumos-edge-runtime — Python example")
    print("=" * 40)

    try:
        # ── 1. Load engine from config ────────────────────────────────────────
        engine = GovernanceEngine.from_config(str(config_path))
        print(f"Engine loaded from: {config_path}\n")

        # ── 2. Evaluate: allowed read by elevated agent ───────────────────────
        raw = engine.evaluate(
            "agent-001",    # agent_id
            "data_read",    # kind — one of the ActionKind variants
            "metrics",      # resource
            0.5,            # estimated_cost in budget units
        )
        decision = parse_decision(raw)
        print("[Case 1] agent-001 reads 'metrics'")
        print(f"  outcome  : {decision['outcome']}")
        print(f"  reason   : {decision['reason']}")
        print()

        # ── 3. Evaluate: denied — restricted agent cannot write ───────────────
        raw = engine.evaluate(
            "agent-002",
            "data_write",
            "config-store",
            1.0,
        )
        decision = parse_decision(raw)
        print("[Case 2] agent-002 writes 'config-store' (restricted level)")
        print(f"  outcome  : {decision['outcome']}")
        print(f"  reason   : {decision['reason']}")
        print()

        # ── 4. Evaluate: denied — action kind requires elevated trust ─────────
        raw = engine.evaluate(
            "agent-002",
            "data_delete",
            "archive",
            2.0,
        )
        decision = parse_decision(raw)
        print("[Case 3] agent-002 deletes from 'archive' (requires elevated)")
        print(f"  outcome  : {decision['outcome']}")
        print(f"  reason   : {decision['reason']}")
        print()

        # ── 5. Evaluate: requires_consent for protected resource ──────────────
        raw = engine.evaluate(
            "agent-001",
            "data_write",
            "user-records",
            1.0,
        )
        decision = parse_decision(raw)
        print("[Case 4] agent-001 writes 'user-records' (consent required)")
        print(f"  outcome  : {decision['outcome']}")
        print(f"  reason   : {decision['reason']}")
        print()

        # ── 6. Grant consent and retry ────────────────────────────────────────
        # The Python binding exposes grant_consent() to record a consent grant
        # in the engine's local storage.  Consent grants are set by a human
        # operator, not inferred automatically.
        engine.grant_consent("agent-001", "user-records", "data_write")

        raw = engine.evaluate(
            "agent-001",
            "data_write",
            "user-records",
            1.0,
        )
        decision = parse_decision(raw)
        print("[Case 5] agent-001 writes 'user-records' after consent grant")
        print(f"  outcome  : {decision['outcome']}")
        print(f"  reason   : {decision['reason']}")
        print()

        # ── 7. Budget exhaustion ──────────────────────────────────────────────
        # agent-001 has a 100-unit budget.  Each external call costs 30 units.
        # After four calls the budget is exceeded.
        print("Exhausting agent-001 budget with external_call (30 units each):")
        for call_number in range(1, 5):
            raw = engine.evaluate(
                "agent-001",
                "external_call",
                "payment-api",
                30.0,
            )
            call_decision = parse_decision(raw)
            print(
                f"  call {call_number}: outcome={call_decision['outcome']}  "
                f"reason={call_decision['reason']!r}"
            )
        print()

        # ── 8. Convenience wrapper from aumos_edge module ─────────────────────
        # The Python shim provides aumos_edge.evaluate() which parses JSON
        # automatically.
        parsed = _aumos_edge.evaluate(
            engine,
            "agent-001",
            "data_read",
            "health-check",
            0.1,
        )
        print("[Case 6] Using aumos_edge.evaluate() convenience wrapper")
        print(f"  type    : {type(parsed)}")
        print(f"  outcome : {parsed.get('outcome')}")
        print()

    finally:
        config_path.unlink(missing_ok=True)

    # ── 9. Sync engine usage ──────────────────────────────────────────────────
    # SyncEngine pushes local audit records and pulls updated config from the
    # remote server.  In offline deployments the sync call is queued and
    # retried when connectivity returns.
    print("Sync engine (connect to a local stub — expected to fail in example):")
    sync_engine = SyncEngine(
        device_id="rpi-edge-001",
        auth_token="<replace-with-real-token>",
    )

    try:
        raw_report = sync_engine.sync("http://localhost:9999/sync")
        report = json.loads(raw_report)
        print(f"  audit_entries_pushed : {report.get('audit_entries_pushed')}")
        print(f"  config_pulled        : {report.get('config_pulled')}")
        print(f"  warnings             : {report.get('warnings')}")
    except RuntimeError as exc:
        # Connection refused is expected here — the example server is not running.
        print(f"  sync failed (expected in example): {exc}")

    print("\nDone.")


if __name__ == "__main__":
    main()
