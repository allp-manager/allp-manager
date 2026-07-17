# Snap Backend

Snap uses two transports behind `SnapService`:

- `SnapdRestService` is primary when the Unix socket is reachable.
- `SnapCliFallbackService` is used only for an absent/denied socket, connection failure, unsupported endpoint, or unrecognized snapd response.

Every fallback records its concrete reason.

## Discovery And Resolution

Wide discovery is `GET /v2/find?q=<encoded-query>&scope=wide`. It creates display candidates only. Exact availability is a separate `GET /v2/find?name=<encoded-name>` after selection.

A recognized snapd error with status `404` and kind `snap-not-found` is authoritative `Unavailable`. It is not eligible for CLI fallback. No sudo or install request may follow it.

Exact metadata normalizes canonical name, publisher verification, confinement, architecture, tracks/channels, stable availability, and installed state. Optional metadata may be absent without turning a valid response into a transport failure.

## Installation

REST installation posts to `/v2/snaps/<encoded-name>`:

```json
{"action":"install","channel":"latest/stable"}
```

Classic confinement adds only `"classic": true`. The returned change ID is polled through `/v2/changes/<id>` until `Done` or another terminal state. Creating a change is not reported as success.

CLI fallback executes direct argv vectors. Classic metadata produces `snap install <name> --classic`; strict metadata omits the flag.

## Recovery

`Search again` includes Snap and reruns discovery. `Try another installer` discards cached candidates and excludes Snap. Diagnostics show transport, socket or executable path, fallback reason, separate discovery/exact requests, status, and bounded raw output.
