#!/usr/bin/env bash
set -euo pipefail

cargo package -p appstruct-ir --locked

cargo package -p appstruct-compiler --locked \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"'

cargo package -p appstruct-migrate --locked \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"'

cargo package -p appstruct-codegen --locked \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-migrate.path="crates/appstruct-migrate"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"'

cargo package -p appstruct-cli --locked \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-migrate.path="crates/appstruct-migrate"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"' \
  --config 'patch.crates-io.appstruct-codegen.path="crates/appstruct-codegen"'
