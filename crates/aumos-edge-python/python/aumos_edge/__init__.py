# SPDX-License-Identifier: BSL-1.1
# Copyright (c) 2026 MuVeraAI Corporation
"""
aumos_edge — Python bindings for the Aumos edge governance runtime.

Build the native extension first:

    pip install maturin
    cd crates/aumos-edge-python
    maturin develop --features python

Then import normally:

    from aumos_edge import GovernanceEngine, SyncEngine

    engine = GovernanceEngine.from_config("edge-config.toml")
    decision_json = engine.evaluate("agent-001", "data_read", "sensor-data", 0.1)
"""

from __future__ import annotations

import json
import sys
from typing import Any

# The native extension is compiled by maturin.  If it is not present we raise
# a clear error rather than silently importing a stub.
try:
    from .aumos_edge import GovernanceEngine, SyncEngine  # type: ignore[import]
except ImportError as exc:
    raise ImportError(
        "aumos_edge native extension not found. "
        "Build it with: maturin develop --features python"
    ) from exc

__all__ = ["GovernanceEngine", "SyncEngine"]


def evaluate(
    engine: "GovernanceEngine",
    agent_id: str,
    kind: str,
    resource: str,
    estimated_cost: float,
) -> dict[str, Any]:
    """Convenience wrapper — evaluate an action and return a parsed dict."""
    raw: str = engine.evaluate(agent_id, kind, resource, estimated_cost)
    return json.loads(raw)


def sync(sync_engine: "SyncEngine", server_url: str) -> dict[str, Any]:
    """Convenience wrapper — run a sync cycle and return a parsed report dict."""
    raw: str = sync_engine.sync(server_url)
    return json.loads(raw)
