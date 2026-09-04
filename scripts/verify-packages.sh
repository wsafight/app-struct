#!/usr/bin/env bash
set -euo pipefail

# This is a pre-commit gate, so validate the current working tree rather than requiring a commit.
package_args=(--locked --allow-dirty)

cargo package -p appstruct-contracts "${package_args[@]}"

cargo package -p appstruct-ir "${package_args[@]}" \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"'

cargo package -p appstruct-module-sdk "${package_args[@]}" \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"'

cargo package -p appstruct-runtime "${package_args[@]}" \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"'

cargo package -p appstruct-compiler "${package_args[@]}" \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"'

cargo package -p appstruct-migrate "${package_args[@]}" \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"'

cargo package -p appstruct-codegen "${package_args[@]}" \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-migrate.path="crates/appstruct-migrate"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"' \
  --config 'patch.crates-io.appstruct-runtime.path="crates/appstruct-runtime"'

cargo package -p appstruct-cli "${package_args[@]}" \
  --config 'patch.crates-io.appstruct-contracts.path="crates/appstruct-contracts"' \
  --config 'patch.crates-io.appstruct-ir.path="crates/appstruct-ir"' \
  --config 'patch.crates-io.appstruct-migrate.path="crates/appstruct-migrate"' \
  --config 'patch.crates-io.appstruct-compiler.path="crates/appstruct-compiler"' \
  --config 'patch.crates-io.appstruct-codegen.path="crates/appstruct-codegen"' \
  --config 'patch.crates-io.appstruct-module-sdk.path="crates/appstruct-module-sdk"' \
  --config 'patch.crates-io.appstruct-runtime.path="crates/appstruct-runtime"'
