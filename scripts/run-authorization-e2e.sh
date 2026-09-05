#!/usr/bin/env bash
set -euo pipefail

: "${APPSTRUCT_E2E_DATABASE_URL:?Set a dedicated PostgreSQL test database URL}"
export APPSTRUCT_ACCESS_TEST_DATABASE_URL="$APPSTRUCT_E2E_DATABASE_URL"
cargo test --locked -p appstruct-codegen --lib generated_scopes_match_policy_truth_tables -- --nocapture
