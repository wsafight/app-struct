#!/usr/bin/env bash
set -euo pipefail

cargo package -p appstruct-contracts --locked

cargo package -p appstruct-ir --locked \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"'

cargo package -p appstruct-module-sdk --locked \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"'

cargo package -p appstruct-runtime --locked \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"'

cargo package -p appstruct-compiler --locked \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"'

cargo package -p appstruct-migrate --locked \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"'

cargo package -p appstruct-codegen --locked \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-migrate.path="crates/appstruct-migrate"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"' \
  --config 'patch.crates-io.appstruct-runtime.path="crates/appstruct-runtime"'

cargo package -p appstruct-cli --locked \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-migrate.path="crates/appstruct-migrate"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"' \
  --config 'patch.crates-io.appstruct-codegen.path="crates/appstruct-codegen"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"' \
  --config 'patch.crates-io.appstruct-runtime.path="crates/appstruct-runtime"'
