# Standalone Wallet v1 — QA Checklist (QBL-233)

The pre-tag manual QA pass. Run through this on every supported OS
(Mac, Windows, Linux) for each release tag. Most non-HW scenarios are
covered by `cargo test --test regtest_e2e -- --ignored`; this doc
covers what automation can't reach: real hardware wallets, OS-specific
behaviour, visual confirmation of broadcasts, and cross-cutting state
preservation.

Copy this file into a Linear comment on QBL-233 when running the pass,
fill in `[ ]` → `[x]` as you go, paste screenshots inline for the
broadcast-success rows, and link any bugs you file as separate
Bug-labeled tickets blocking QBL-233.

---

## Environment

- [ ] **Host OS:** _(macOS 14.x / Windows 11 / Ubuntu 22.04 / ...)_
- [ ] **Build:** _(commit sha or tag of the desktop build)_
- [ ] **Network endpoints:**
  - Regtest electrs: `ssl://ers.regtest.sigvault.org:443`
  - Testnet4: _(the public default from QBL-231 settings)_
- [ ] **Hardware devices connected:**
  - [ ] Ledger Nano S Plus _(firmware version: ___)_
  - [ ] BitBox02 Multi _(firmware version: ___)_
  - [ ] Blockstream Jade _(firmware version: ___)_
  - [ ] Coldcard Mk4 or Q _(firmware version: ___)_
- [ ] **Automated test pass (regtest):**
      `cargo test --test regtest_e2e -- --ignored --nocapture` — all green?

---

## Automated coverage (already in `tests/regtest_e2e.rs`)

Run these once on regtest before touching the manual matrix below;
they catch the easy-to-automate breakage on the wallet creation,
recovery, and signing paths.

| Test | What it proves |
|------|----------------|
| `singlesig_hot_full_cycle` | Hot singlesig: create → fund → sync → send → broadcast lands in mempool |
| `watch_only_descriptor_import` | Watch-only wallet derives same addresses as the source hot wallet and observes balance after sync |
| `bip39_passphrase_round_trip` | Cosigner recovery with BIP39 25th-word passphrase passes fingerprint check; wrong passphrase rejected |
| `lock_unlock_cycle_preserves_state` | Lock + re-unlock returns the same address sequence |
| `wrong_passphrase_rejected` | Wrong passphrase fails cleanly, correct passphrase still works after |
| `app_restart_preserves_wallets` | New manager instance against the same data dir lists prior wallets and re-unlocks |
| `mainnet_creation_gated` | Mainnet creation rejected with `UnsupportedNetwork` |
| `unspendable_primary_descriptor_uses_nums` | QBL-235 unspendable primary substitution flags wallet as `recovery_only` |

---

## Per-policy manual matrix (HW-required)

### Singlesig HW (Ledger / BitBox / Jade / Coldcard, one row per device)

| Device | Regtest | Testnet4 |
|--------|---------|----------|
| Ledger | [ ] Create → receive → fund → send → broadcast — screenshot of `txid` from broadcast confirmation | [ ] (same) |
| BitBox | [ ] (same) | [ ] (same) |
| Jade | [ ] (same) | [ ] (same) |
| Coldcard | [ ] PSBT export via .psbt file (QBL-234 air-gapped flow) — sign offline → import signed → broadcast | [ ] (same) |

Confirms: device discovery, xpub extraction, on-device policy
registration (Ledger HMAC / BitBox script-config / Jade descriptor),
PSBT signing path the device understands.

### 2-of-3 multisig

| Cosigner mix | Regtest | Testnet4 |
|--------------|---------|----------|
| 3× HW (Ledger + BitBox + Jade) | [ ] Create → fund → collect 2 sigs via USB → broadcast | [ ] (same) |
| 2× HW + 1× hot (recovered via cosigner recovery, QBL-230) | [ ] (same) | [ ] (same) |
| 2× HW + 1× air-gapped Coldcard via .psbt | [ ] (same) — file round-trip | [ ] (same) |

Cross-check after creation: each HW device registers the policy on
first signing prompt and doesn't re-prompt on subsequent signs in the
same session.

### Liana (timelocked-recovery)

| Configuration | Regtest | Testnet4 |
|---------------|---------|----------|
| Primary HW + recovery HW (6mo timelock) | [ ] Create → primary spend before timelock → broadcast | [ ] (same) |
| Primary HW + recovery HW — recovery path after timelock | [ ] Mine 26,280+ blocks on regtest, sign with recovery key only, broadcast | [ ] N/A on testnet4 |
| Unspendable primary (QBL-235) + recovery HW | [ ] Create wallet, wallet card shows "Recovery-only" badge, dashboard banner explains constraint | [ ] (same) |

### Watch-only (descriptor import)

| Source | Regtest | Testnet4 |
|--------|---------|----------|
| Sparrow-exported descriptor | [ ] Import → sync → addresses match Sparrow's, balance matches | [ ] (same) |
| Specter-exported descriptor | [ ] (same) | [ ] (same) |
| Descriptor-spendable mode (QBL-234) | [ ] Import with "Enable spending with hardware wallet" toggle on → Send button surfaces on dashboard → sign via connected HW | [ ] (same) |

### QR PSBT transports (QBL-234)

| Signer | Regtest | Testnet4 |
|--------|---------|----------|
| SeedSigner via animated BBQr | [ ] Show QR → device scans → device returns signed via QR → camera scan reassembles → broadcast | [ ] (same) |
| Sparrow / Foundation Passport via animated UR | [ ] (same — UR format) | [ ] (same) |

---

## Cross-cutting checks (manual on each OS)

- [ ] **App restart preserves state:** create + fund a wallet, close the app, reopen — wallet list shows the wallet, balance + history persist after re-unlock.
- [ ] **Mode switch round-trip:** in Local mode create wallet A. Switch to Cloud mode. Switch back to Local. Wallet A is still there with same balance/history.
- [ ] **Mainnet UI gating:** every wallet-creation entry point (singlesig hot, singlesig HW, multisig, Liana, watch-only, recovery) — does it surface mainnet as a network option? It should NOT (gated by QBL-232).
- [ ] **Network mismatch caught client-side:** in a regtest wallet, paste a testnet4 address in the Send recipient field → error surfaces before PSBT build.
- [ ] **Wrong passphrase rejected:** in the unlock step, try a wrong passphrase 3× → each attempt fails cleanly with no state mutation (lock the wallet, try again with correct passphrase, succeeds).
- [ ] **HW unplug mid-flow:** in the middle of signing, unplug the device. UI surfaces a clean error. Re-plug, try again, succeeds.
- [ ] **Window controls:** drag, minimize, close — all behave normally on each OS. Resize handles work.

---

## Bugs filed

Track each non-blocker bug as a separate Linear ticket with the `Bug`
label and link it back to QBL-233. Blockers go in the table below.

| Ticket | Severity | Description |
|--------|----------|-------------|
|        |          |             |

---

## Sign-off

- [ ] All automated tests green on regtest.
- [ ] All HW matrix rows green on at least one OS each, with screenshots attached.
- [ ] All cross-cutting checks pass on macOS, Windows, Linux.
- [ ] No Severity-1 blocker bugs open against QBL-233.

Pass: ___________  Date: ___________  Tagged release: ___________
