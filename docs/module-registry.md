# Remote Module Registry

Remote modules are installed explicitly with the CLI and then consumed offline by the compiler:

```bash
appstruct module install vendor/analytics@1.2.3 \
  --registry https://registry.example.com \
  --public-key BASE64_ED25519_PUBLIC_KEY
appstruct module list
appstruct module verify
appstruct module update vendor/analytics@1.2.4
appstruct module uninstall vendor/analytics
```

Installation verifies the envelope SHA-256 and Ed25519 signature, checks the AppStruct and Module
API compatibility versions, validates every manifest/artifact path, and writes the package under
`modules/.registry/`. The committed `appstruct.modules.lock` records the registry, public key,
package and manifest digests, cache paths, and compatibility versions. Normal `check`, `generate`,
`build`, and runtime startup never contact the registry and reject missing or modified cache files.

The signed payload contains the module manifest and base64-encoded artifacts. The envelope signs
the exact decoded payload bytes. Remote modules remain static artifacts with a no-op runtime
starter; dynamically loading third-party Rust libraries is not supported.

`verify` is offline and rechecks the locked signature, compatibility metadata, manifest, and every
cached artifact. Pass a module name to verify only that entry. `update` requires an explicit target
version and reuses the locked registry and public key unless `--registry` or `--public-key` is
provided. A failed download or signature check leaves the previous lock entry active. `uninstall`
removes the lock entry and then deletes its unreferenced content-addressed cache directory.
