#!/usr/bin/env bash
# SPDX-License-Identifier: BSL-1.1
# Copyright (c) 2026 MuVeraAI Corporation
#
# fire-line-audit.sh — Static scan for forbidden identifiers and structural violations.
# Run this in CI or before every commit.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATES_DIR="${REPO_ROOT}/crates"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

VIOLATIONS=0

log_ok()   { echo -e "${GREEN}[OK]${NC}    $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC}  $1"; }
log_fail() { echo -e "${RED}[FAIL]${NC}  $1"; VIOLATIONS=$((VIOLATIONS + 1)); }

echo "================================================================"
echo "  aumos-edge-runtime  — FIRE LINE AUDIT"
echo "  $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
echo "================================================================"
echo ""

# ── 1. Forbidden identifiers ──────────────────────────────────────────────────
FORBIDDEN_IDS=(
  "progressLevel"
  "promoteLevel"
  "computeTrustScore"
  "behavioralScore"
  "adaptiveBudget"
  "optimizeBudget"
  "predictSpending"
  "detectAnomaly"
  "generateCounterfactual"
  "PersonalWorldModel"
  "MissionAlignment"
  "SocialTrust"
  "CognitiveLoop"
  "AttentionFilter"
  "GOVERNANCE_PIPELINE"
)

echo "--- Checking forbidden identifiers ---"
for id in "${FORBIDDEN_IDS[@]}"; do
  MATCHES=$(grep -r --include="*.rs" --include="*.py" --include="*.ts" \
    -l "${id}" "${CRATES_DIR}" 2>/dev/null || true)
  if [[ -n "${MATCHES}" ]]; then
    log_fail "Forbidden identifier '${id}' found in:"
    while IFS= read -r file; do
      echo "        ${file}"
    done <<< "${MATCHES}"
  else
    log_ok "No use of '${id}'"
  fi
done
echo ""

# ── 2. SPDX headers ──────────────────────────────────────────────────────────
echo "--- Checking SPDX headers ---"
while IFS= read -r -d '' file; do
  if ! head -2 "${file}" | grep -q "SPDX-License-Identifier: BSL-1.1"; then
    log_fail "Missing SPDX header: ${file}"
  fi
done < <(find "${CRATES_DIR}" -name "*.rs" -print0)

while IFS= read -r -d '' file; do
  if ! head -2 "${file}" | grep -q "SPDX-License-Identifier: BSL-1.1"; then
    log_fail "Missing SPDX header: ${file}"
  fi
done < <(find "${CRATES_DIR}" -name "*.py" -print0)

log_ok "SPDX header scan complete"
echo ""

# ── 3. Forbidden module imports ───────────────────────────────────────────────
echo "--- Checking for forbidden module imports ---"
FORBIDDEN_MODULES=("pwm" "mae" "stp" "cognitive_loop" "aumos_pwm" "aumos_mae" "aumos_stp")
for mod in "${FORBIDDEN_MODULES[@]}"; do
  MATCHES=$(grep -r --include="*.rs" -l "use ${mod}" "${CRATES_DIR}" 2>/dev/null || true)
  if [[ -n "${MATCHES}" ]]; then
    log_fail "Forbidden import 'use ${mod}' found in:"
    while IFS= read -r file; do
      echo "        ${file}"
    done <<< "${MATCHES}"
  else
    log_ok "No import of '${mod}'"
  fi
done
echo ""

# ── 4. No on-device inference markers ─────────────────────────────────────────
echo "--- Checking for on-device inference ---"
INFERENCE_PATTERNS=("candle" "llm_rs" "tch::" "tract_" "ort::" "fastembed")
for pat in "${INFERENCE_PATTERNS[@]}"; do
  MATCHES=$(grep -r --include="*.rs" --include="*.toml" -l "${pat}" "${REPO_ROOT}" 2>/dev/null || true)
  if [[ -n "${MATCHES}" ]]; then
    log_fail "Inference dependency '${pat}' found in:"
    while IFS= read -r file; do
      echo "        ${file}"
    done <<< "${MATCHES}"
  else
    log_ok "No use of '${pat}'"
  fi
done
echo ""

# ── Summary ───────────────────────────────────────────────────────────────────
echo "================================================================"
if [[ "${VIOLATIONS}" -eq 0 ]]; then
  echo -e "${GREEN}FIRE LINE AUDIT PASSED — 0 violations${NC}"
  exit 0
else
  echo -e "${RED}FIRE LINE AUDIT FAILED — ${VIOLATIONS} violation(s)${NC}"
  exit 1
fi
