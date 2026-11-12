# Snap Backend

Snap uses two transports behind `SnapService`:

- `SnapdRestService` is primary when the Unix socket is reachable.
- `SnapCliFallbackService` is used only for an absent/denied socket, connection failure, unsupported endpoint, or unrecognized snapd response.

Every fallback records its concrete reason.

Transient socket read timeouts or disconnects during discovery and exact lookup
may retry the same read through the native CLI. Store queries have a bounded
45-second CLI deadline. An authoritative `snap-not-found` response still blocks
installation, and a lost mutation response never authorizes replaying an install.

Snap health is capability-specific:

- daemon
- discovery
- metadata resolution
- new installation
- installed snap refresh

A degraded or unavailable new-installation path must not block refreshing already-installed snaps. Likewise, a successful `snap refresh` must not be reported as proof that new Snap installation is operational.

## Discovery And Resolution

Wide discovery is `GET /v2/find?q=<encoded-query>&scope=wide`. It creates display candidates only. Exact availability is a separate `GET /v2/find?name=<encoded-name>` after selection.

A recognized snapd error with status `404` and kind `snap-not-found` is authoritative `Stale`: wide discovery listed the snap, but exact installable metadata cannot resolve it. It is not eligible for CLI fallback because `snap info` and `snap install` use exact name resolution too. No sudo or install request may follow stale metadata, even if wide discovery returned the snap as a search result.

Exact metadata normalizes canonical name, publisher verification, confinement, architecture, tracks/channels, stable availability, and installed state. Optional metadata may be absent without turning a valid response into a transport failure.

## Installation

REST installation posts to `/v2/snaps/<encoded-name>`:

```json
{"action":"install","channel":"latest/stable"}
```

Classic confinement adds only `"classic": true`. The returned change ID is polled through `/v2/changes/<id>` until `Done` or another terminal state. Creating a change is not reported as success.

CLI fallback executes direct argv vectors. Classic metadata produces `snap install <name> --classic`; strict metadata omits the flag.

## Maintenance

Snap does not expose a useful metadata-only operation for Allp's `update` semantics. `allp update` reports Snap as `Not applicable` and does not run `snap refresh`.

Installed snap refresh is a combined refresh/upgrade operation. `allp upgrade` plans one `snap refresh` operation and parses `All snaps up to date.` as an up-to-date result.

HTTP errors and response timeouts from the Snap Store refresh endpoint are
reported with their native evidence. A passing `snap debug connectivity` check
does not prove that `POST /v2/snaps/refresh` will succeed. Allp never rewrites
snapd's network configuration or forces an operation to appear successful.
Install/remove failures use `Failed`; only real package-management lock conflicts
use `Busy`. Both stdout and stderr are retained when the native command fails.

## Recovery

`Search again` includes Snap and reruns discovery. `Try another installer` discards cached candidates and excludes Snap. Diagnostics show transport, socket or executable path, fallback reason, separate discovery/exact requests, status, and bounded raw output.
