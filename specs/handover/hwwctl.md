# Handover: e2e hardware-wallet integration via hwwctl

Hands off the in-progress work to wire **real BitBox02 emulator
flows** into the desktop e2e suite. The foundation is landed and
the upstream release pipeline now ships both the daemon binary AND
the BitBox02 simulator bundle, so the only thing left is writing
the first spec against the desktop's add-device UI — plus fixing
the pairing-cache branch in the desktop's `WithLocalCache` path
that's currently blocking the flow.

Read this whole doc before touching the spec — it'll save you the
discovery loop.

## What hwwctl is

`hwwctl` (<https://github.com/n1rna/hwwctl>, full docs at
<https://hwwctl.n1rna.net>) is a CLI + daemon that spawns
hardware-wallet **emulators** behind a real `/dev/uhid` virtual HID
device. From the desktop's perspective, it looks like a physically
plugged-in BitBox02 — `hidapi`'s `device_list()` enumerates it
with the correct VID/PID and a per-instance `serial_number`.

```
sigvault-desktop ──(hidapi)──▶ /dev/hidrawN
                                   ▲
                                   │ kernel hotplug from
                                   │
                              /dev/uhid ◀── hwwctl daemon ──▶ bitbox02-simulator (TCP)
```

**Pinned release: `hwwctl-v0.1.2`.** Earlier versions have a daemon
shutdown race that drops the `shutting_down` response on the floor
(fixed in 0.1.2). v0.1.0 has no bundle asset; v0.1.1 has the bundle
but the buggy shutdown.

Release assets are at
<https://github.com/n1rna/hwwctl/releases/tag/hwwctl-v0.1.2>:

- `hwwctl-linux-x86_64.tar.gz` (+ `.sha256`) — the daemon binary.
- `hwwctl-bitbox02-linux-x86_64.tar.gz` (+ `.sha256`) — BitBox02
  simulator bundle. Ships a `manifest.json` so it's install-ready
  with a plain `tar -x --strip-components=1`.

## What's already landed

### `n1rna/hwwctl` — on `main`

- **PR #2** — daemon, CLI, IPC protocol, BitBox02 wired with unique
  HID serials, per-instance log buffer + bridge stats, repo renamed
  from `hwwtui`, TUI deleted.
- **PR #3** — docs site at <https://hwwctl.n1rna.net> (Astro +
  Starlight, loads content from `docs/` in the repo).
- **PR #4** — BitBox02 simulator bundle attached to releases. Bundle
  build is in the same release workflow as the binary, cache-shared
  with the smoke workflow so it's fast on hot caches.
- **PR #5** — `bitbox02_full_e2e_via_uhid_bridge` test unbroken; it
  now runs cleanly on any Linux host with a BB02 bundle + world-rw
  hidraw. This was the missing verification that `wait_confirm`
  works over the UHID bridge (it does — see the "Known pairing
  cause" section below).
- Hotfix on main — daemon shutdown race that v0.1.1 exposed (smoke
  in CI was racing `process::exit` against `write_frame`); fixed in
  v0.1.2.

### `n1rna/sigvault` — branch `feat/e2e-hwwctl-integration` (you start here)

- `e2e/fixtures/hwwctl.ts` — typed wrapper. `Hwwctl` class with
  `start()`, `stop()`, `status()`, `logs()`, `bridgeStats()`,
  `shutdown()`. Auto-spawns the daemon on first command. Throws
  `HwwctlError` with a stable `ErrorCode` string (`BUNDLE_MISSING`,
  `BRIDGE_FAILED`, `INSTANCE_NOT_FOUND`, …) — match on `e.code`, not
  message text.
- `e2e/docker/Dockerfile` — downloads the `hwwctl` binary into
  `/usr/local/bin/hwwctl`. **Currently pins `HWWCTL_VERSION=v0.1.0`;
  you need to bump this to `v0.1.2` AND add the bundle download —
  see "First task" below.**
- `e2e/docker/run.sh` — passes `--device=/dev/uhid` through when the
  host has it; warns or fails (`E2E_REQUIRE_UHID=1`) when missing.
- `.github/workflows/e2e-docker-build.yaml` — PR smoke that rebuilds
  the e2e image to catch a broken hwwctl pin / moved release URL.

### Not yet done

- **The actual add-device spec.** No `e2e/specs/desktop/add-device-*`
  exists. This is the main work item.
- **The desktop-side pairing-cache fix.** See "Known pairing cause"
  below — the desktop calls `wait_confirm()` unconditionally after
  `PairingBitbox02WithLocalCache::connect()`. On a cache hit the
  handle is already paired; `wait_confirm()` then errors with
  "wrong state". Fix belongs in the desktop's add-device flow, not
  in hwwctl.
- The host CI environment that runs the WDIO suite (`e2e-desktop.yaml`
  in the sigvault repo) doesn't `sudo modprobe uhid && sudo chmod 666
  /dev/uhid` and install a `SUBSYSTEM=="hidraw", MODE="0666"` udev
  rule. Both are prerequisites for hidapi opening the device.

## First task: bump the Dockerfile to v0.1.2 + install the bundle

Apply this diff to `e2e/docker/Dockerfile` on `feat/e2e-hwwctl-integration`:

```diff
-ARG HWWCTL_VERSION=v0.1.0
+ARG HWWCTL_VERSION=v0.1.2
 RUN set -eux; \
-    url="https://github.com/n1rna/hwwctl/releases/download/hwwctl-${HWWCTL_VERSION}/hwwctl-linux-x86_64.tar.gz"; \
-    curl -fsSL "$url" -o /tmp/hwwctl.tar.gz; \
-    curl -fsSL "${url}.sha256" -o /tmp/hwwctl.sha256; \
-    cd /tmp && sha256sum -c hwwctl.sha256; \
-    tar -C /usr/local/bin -xzf /tmp/hwwctl.tar.gz; \
-    rm -f /tmp/hwwctl.tar.gz /tmp/hwwctl.sha256; \
+    base="https://github.com/n1rna/hwwctl/releases/download/hwwctl-${HWWCTL_VERSION}"; \
+    # 1. daemon binary
+    curl -fsSL "$base/hwwctl-linux-x86_64.tar.gz" -o /tmp/hwwctl.tar.gz; \
+    curl -fsSL "$base/hwwctl-linux-x86_64.tar.gz.sha256" -o /tmp/hwwctl.sha256; \
+    (cd /tmp && sha256sum -c hwwctl.sha256); \
+    tar -C /usr/local/bin -xzf /tmp/hwwctl.tar.gz; \
+    # 2. BitBox02 simulator bundle — extract into the daemon's
+    # standard bundle dir so `hwwctl start bitbox02` works out of
+    # the box. The tarball ships a manifest.json, so no extra
+    # post-install step is needed.
+    mkdir -p /root/.hwwctl/bundles/bitbox02; \
+    curl -fsSL "$base/hwwctl-bitbox02-linux-x86_64.tar.gz" -o /tmp/bb02.tar.gz; \
+    curl -fsSL "$base/hwwctl-bitbox02-linux-x86_64.tar.gz.sha256" -o /tmp/bb02.sha256; \
+    (cd /tmp && sha256sum -c bb02.sha256); \
+    tar -xzf /tmp/bb02.tar.gz --strip-components=1 -C /root/.hwwctl/bundles/bitbox02; \
+    rm -f /tmp/hwwctl.tar.gz /tmp/hwwctl.sha256 /tmp/bb02.tar.gz /tmp/bb02.sha256; \
     hwwctl --version
```

Verify:

```bash
cd sigvault/e2e/docker
./run.sh --rebuild shell
# inside:
hwwctl --version           # → 0.1.2
hwwctl ping                # auto-spawns daemon; → pong
hwwctl start bitbox02      # → started; hidraw populated
hwwctl stop bitbox02-…
```

If the host doesn't have `/dev/uhid`, `start bitbox02` will return
`BRIDGE_FAILED`. That's how you know the host needs `modprobe uhid`.

## How to run locally (Linux)

```bash
# Host: load uhid + open access
sudo modprobe uhid
sudo tee /etc/udev/rules.d/70-hwwctl.rules >/dev/null <<'RULE'
KERNEL=="uhid", MODE="0666"
SUBSYSTEM=="hidraw", MODE="0666"
RULE
sudo udevadm control --reload-rules && sudo udevadm trigger
sudo chmod 0666 /dev/uhid

# Build / use the desktop binary (existing flow)
cd sigvault/e2e/docker
./run.sh build      # builds sigvault-desktop in the container

# Run the suite (passes /dev/uhid through)
./run.sh wdio

# Or get a shell to poke at it
./run.sh shell
# inside:
hwwctl ping
hwwctl start bitbox02
hwwctl status
hwwctl stop bitbox02-…
hwwctl shutdown
```

macOS dev path: the e2e harness already runs in Docker on macOS, but
`/dev/uhid` doesn't exist on Darwin. You can run the suite without
hwwctl-dependent specs (the run.sh prints a warning); for hardware-
wallet specs you need a Linux host.

## Known pairing cause (why the current desktop code fails)

The desktop calls:

```rust
let pairing = PairingBitbox02WithLocalCache::connect(...).await?;
// UI displays pairing.pairing_code() and waits for user to click Confirm
let paired = pairing.wait_confirm().await?;   // ← "wrong state" error here
```

**We verified upstream that `wait_confirm()` works fine over the
UHID bridge** — the `bitbox02_full_e2e_via_uhid_bridge` test in
hwwctl/PR #5 exercises exactly this path with `NoiseConfigNoCache`
and passes end-to-end. So the simulator implements HID-side pairing
confirmation correctly.

The desktop's failure is specific to `WithLocalCache`. When the
local cache has a matching noise static key from a prior run,
`connect()` short-circuits the pending-confirmation state — the
returned handle is already-paired, no on-device confirmation is
pending, and `wait_confirm()` on it hits "wrong state" because
there's nothing left to confirm.

**Fix**: after `WithLocalCache::connect()`, branch on the returned
pairing state before calling `wait_confirm()`. `bitbox-api` exposes
this — the return has variants / methods for "already paired" vs
"needs confirmation". The desktop currently treats every return as
"needs confirmation".

Quick verification path for whoever picks this up: wipe the desktop
side of the pairing cache and re-run the add-device flow. The
first run after cache-wipe should hit the fresh-pairing branch and
succeed. If it does, the diagnosis is confirmed and the fix is
"skip wait_confirm on already-paired handles".

## Where the desktop's add-device flow lives

The TS / Tauri-command shape that the spec must drive:

| Layer | File | Key entry point |
|---|---|---|
| React UI | `src/components/DeviceDiscovery.tsx` | renders the "Discover Devices" list, has "Add" / "Unlock" buttons |
| React UI | `src/components/DeviceCreationSession.tsx` | step machine after a device is selected |
| Tauri command | `src-tauri/src/commands/hwi.rs::cmd_discover_hardware_wallets` | calls async-hwi / bitbox-api / coldcard to enumerate |
| Tauri command | `src-tauri/src/commands/hwi.rs::cmd_unlock_device` | pairs / unlocks (BitBox02 noise handshake) — this is where the WithLocalCache bug lives |
| Tauri command | `src-tauri/src/commands/hwi.rs::cmd_get_device_xpub` | reads xpub at the requested derivation path |
| Tauri command | `src-tauri/src/commands/hwi.rs::cmd_submit_device_registration` | POSTs to the sigvault API → device → ACTIVE |

The full happy path that the spec should drive (replaces
`seedActiveUserDevice` for BitBox02):

1. `hwwctl.start("bitbox02")` → kernel exposes `/dev/hidrawN`
2. UI: click "Add device" → React calls `cmd_discover_hardware_wallets`
3. Spec: assert the BitBox02 (filter by `serial === inst.serial`)
   appears in the list and click it
4. UI: `cmd_unlock_device` → bitbox-api pairs (simulator auto-confirms
   on the *device* side, but the desktop must not call `wait_confirm`
   when the WithLocalCache path returned an already-paired handle —
   see "Known pairing cause" above)
5. UI: `cmd_get_device_xpub` → returns an xpub
6. UI: user submits → `cmd_submit_device_registration` → device goes
   ACTIVE
7. Spec: assert `device.status === "ACTIVE"` via the API client (same
   pattern as `fixtures/devices.ts::seedActiveUserDevice`)
8. `hwwctl.stop(inst.instance)`

### Constraints / gotchas

- **BitBox02 simulator is single-client.** Once the bridge attaches,
  *only* that session can talk to the device. Don't try to enumerate
  twice; the second `cmd_discover_hardware_wallets` after a successful
  unlock will see a busy device. If the desktop's flow needs to
  re-enumerate, stop+start the instance.
- **Simulator auto-confirms.** There's no PIN prompt and no
  "confirm-on-screen" step on the BitBox02 simulator's device side.
  The `wait_confirm()` state machine is still traversed on the
  fresh-pairing branch, but the "user confirmation" step completes
  immediately.
- **No screen readback.** Unlike Trezor, BitBox02 has no debug-link
  protocol. `hwwctl screen` / `hwwctl press` are deliberately not
  wired for BitBox02 (they return `WALLET_UNSUPPORTED`).
- **Serial filtering matters.** Two parallel WDIO workers could each
  spawn a BitBox02; they'll both appear in `cmd_discover_hardware_wallets`'s
  output. Always filter by `serial === hwwctl.start(...).serial`
  rather than picking the first BitBox02 returned.
- **Shutdown is ordered properly (v0.1.2+).** Earlier versions
  could lose the `shutting_down` response. If you see exit-code-2 on
  `hwwctl shutdown` in CI, double-check the pinned version.

## Suggested spec scaffold

```ts
// e2e/specs/desktop/add-device-bitbox02.spec.ts
import { browser, expect } from "@wdio/globals";
import { Hwwctl } from "../../fixtures/hwwctl";
import { ApiClient } from "../../fixtures/api-client";
import { createVerifiedTestUser } from "../../fixtures/test-user";
import { env } from "../../fixtures/env";

describe("desktop · add BitBox02 device", function () {
  this.timeout(180_000);

  const hww = new Hwwctl();
  let api: ApiClient;
  let token: string;
  let instanceId: string | null = null;

  before(async () => {
    api = new ApiClient(env.apiBaseUrl);
    ({ token } = await createVerifiedTestUser(api));
    // Existing pattern: drive Mode chooser → SelectEnv → login. See
    // desktop-login.spec.ts for the OIDC handoff details.
    await loginIntoDesktop(/* ... */);
  });

  afterEach(async () => {
    if (instanceId) {
      await hww.stop(instanceId).catch(() => {});
      instanceId = null;
    }
  });

  after(async () => {
    await hww.shutdown().catch(() => {});
  });

  it("discovers, unlocks, captures xpub, registers ACTIVE", async () => {
    const inst = await hww.start("bitbox02");
    instanceId = inst.instance;

    // Drive the desktop UI here. See DeviceDiscovery.tsx for the
    // selectors / button text — the React tree uses Tauri-driver's
    // CSS selectors, NOT data-testid in most places.
    // ...

    // Verify against the sigvault API that the device is ACTIVE.
    const devices = await api.listDevices(token);
    const ours = devices.find((d) => d.name.includes(/* test-scoped name */));
    expect(ours?.status).toBe("ACTIVE");
  });
});
```

## What to read next

1. <https://hwwctl.n1rna.net/cli/> — the canonical CLI reference
   (every subcommand, every flag, every error code).
2. `e2e/fixtures/hwwctl.ts` — full TS API surface.
3. `e2e/specs/desktop/desktop-login.spec.ts` — pattern for driving
   the desktop via WDIO + Playwright OIDC handoff. The structure
   for `before(...)` setup will be similar.
4. `e2e/fixtures/devices.ts::seedActiveUserDevice` — the API-only
   path you're replacing for BitBox02.
5. `src/components/DeviceDiscovery.tsx` — pick selectors / button
   text for the WDIO assertions.

## Bumping hwwctl

When the daemon's protocol or behavior changes upstream:

1. Watch <https://github.com/n1rna/hwwctl/releases> for a new
   `hwwctl-vX.Y.Z` tag.
2. Edit `e2e/docker/Dockerfile`'s `ARG HWWCTL_VERSION=v0.1.X`.
   Both the binary and the bundle download from the same release —
   one pin updates both.
3. `docker/run.sh --rebuild` to refresh the image.
4. The PR-time `e2e-docker-build.yaml` will confirm the new pin
   works (curls the binary, runs `hwwctl --version`, runs
   `hwwctl --help` to confirm the expected subcommands are
   present).

If the daemon adds a new request/response shape, also extend
`fixtures/hwwctl.ts` (the `DaemonResponse` union + a new method on
`Hwwctl`).

## Pointers / debugging

- Daemon logs land at `/tmp/hwwctl.log` inside the container. Mount
  `/tmp` out, or `cat` it in the spec's `afterEach` on failure.
- `hwwctl logs <id> --source emulator` shows the bitbox02-simulator
  stdout/stderr (often empty — the simulator is quiet on startup).
- `hwwctl logs <id> --source bridge` shows the HID reports going
  over the wire — good for "did the desktop even talk to the
  device".
- `hwwctl bridge-stats <id>` — subtract two snapshots to confirm
  bytes flowed during a given UI step. If `host_to_device_reports`
  is zero after the UI claims to have unlocked the device, the
  desktop never actually opened the hidraw node.
- The hwwctl docs site at <https://hwwctl.n1rna.net> is the
  canonical reference if the in-repo handover ever drifts.
