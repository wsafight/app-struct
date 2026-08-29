# Remote Module Registry

Remote modules are installed explicitly with the CLI and then consumed offline by the compiler:

```bash
appstruct module install vendor/analytics@1.2.3 \
  --registry https://registry.example.com \
  --public-key BASE64_ED25519_PUBLIC_KEY
appstruct module list
```

Installation verifies the envelope SHA-256 and Ed25519 signature, checks the AppStruct and Module
API compatibility versions, validates every manifest/artifact path, and writes the package under
`modules/.registry/`. The committed `appstruct.modules.lock` records the registry, public key,
package and manifest digests, cache paths, and compatibility versions. Normal `check`, `generate`,
`build`, and runtime startup never contact the registry and reject missing or modified cache files.

The signed payload contains the module manifest and base64-encoded artifacts. The envelope signs
the exact decoded payload bytes. Remote modules remain static artifacts with a no-op runtime
starter; dynamically loading third-party Rust libraries is not supported.
