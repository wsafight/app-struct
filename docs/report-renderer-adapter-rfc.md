# Production Report Renderer Adapter RFC

> Status: preview implementation available; Linux isolation acceptance pending
> Date: 2026-09-05

## Outcome

The implemented v1 contract and verification status are in [report-renderer.md](report-renderer.md).
That document supersedes the proposed transport and resource budgets below: v1 uses a private Unix
socket, a 2 MiB HTML/inline-asset bundle and a 128-task container limit (Linux tasks include browser
threads). Remote services and separate asset bundles remain future work.

`capture-v1` remains the deterministic default renderer for development and simple text reports.
A browser-capable production renderer must run outside the API process and outside the generated
Job worker's trust boundary. This RFC defines the safety and lifecycle contract required before an
adapter can be selected in production.

## Trust Model

The first production version renders only templates shipped with the application source and locked
by the generated artifact digest. Tenant-uploaded HTML, JavaScript, fonts, or executable templates
are not accepted. Report input remains the encrypted, size-bounded JSON snapshot created by the
current Report API.

The renderer receives a fully resolved document request. It has no database credentials, session
cookies, cloud credentials, tenant secrets, or direct File provider write access. The Job worker
publishes successful bytes through the existing tenant-bound File module after validating the
response.

## Isolation

The renderer runs as a dedicated process, container, or remote service under a non-root identity.
Its filesystem is read-only except for a fresh per-run temporary directory. Linux deployments use
a seccomp profile, dropped capabilities, a PID limit, and a memory limit. Browser sandboxing is
required but is not treated as the outer security boundary.

Outbound network access is denied by default at the operating-system or network-policy layer. The
renderer rejects `file:`, `data:` documents above the byte budget, loopback, link-local, private,
multicast, Unix socket, and cloud metadata targets. Redirects and every resolved address are checked
again to prevent DNS rebinding. Version 1 has no remote-resource allowlist; fonts, styles, and images
must be application artifacts included in the request bundle.

## Adapter Request

Each request binds these immutable values:

- ReportRun ID, tenant ID if present, template name/version, and artifact digest;
- renderer protocol version and renderer implementation version;
- locale, timezone, paper, orientation, and the decrypted JSON snapshot;
- an absolute deadline and a cancellation token scoped to this run;
- content digests for every bundled HTML, CSS, font, and image artifact.

The transport is authenticated and integrity protected. A local process may use an inherited pipe
or Unix socket with peer credentials. A remote service requires mutually authenticated TLS and a
short-lived request signature. Requests and logs never contain the snapshot encryption key.

## Resource Budget

The initial maximums are part of the adapter configuration and must only be lowered per deployment:

| Budget | Maximum |
| --- | ---: |
| JSON snapshot | existing Report `max_input_bytes`, at most 1 MiB |
| resolved HTML | 2 MiB |
| bundled assets | 20 MiB total, 5 MiB each |
| rendered pages | 100 |
| page dimension | 2,000 by 2,000 mm |
| output PDF | 50 MiB |
| wall clock | 30 seconds |
| browser processes | 16 per run |
| memory | 512 MiB per run |
| temporary storage | 128 MiB per run |

The Job lease must be renewed while rendering and remain valid beyond the hard deadline plus a
publication margin. Losing the lease or receiving cancellation terminates the renderer process,
removes its temporary directory, and prevents publication. Cancellation is checked before launch,
during rendering, after result validation, and immediately before the atomic File publish.

## Results And Failures

Success returns PDF bytes or a private temporary handle, media type, byte length, page count, and a
SHA-256 digest. The worker verifies all declared values and atomically publishes exactly one object.
Retries for the same ReportRun may reuse a verified published object, matching the current
idempotent publication behavior.

Failures map to bounded stable codes: invalid template artifact, blocked resource, render timeout,
resource limit, browser crash, invalid output, cancelled, and adapter unavailable. Detailed browser
errors remain in redacted operator logs and are never returned to tenants. Retryability is decided
by code, not by parsing messages.

## Operations And Acceptance

Metrics include queue delay, render duration, cancellation latency, retry count, output bytes,
resource-limit terminations, and adapter availability, tagged only with bounded template/version
labels. Logs bind run and tenant IDs but redact report input and rendered content.

Acceptance requires integration tests for network denial, DNS rebinding, redirects, `file:` access,
metadata endpoints, oversized assets/output, memory and PID exhaustion, timeout, cancellation at
each lifecycle phase, lease loss, crash cleanup, retry idempotency, and tenant-safe publication.
No production adapter may be enabled until these tests run in the same isolation environment used
for deployment.
