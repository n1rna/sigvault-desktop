# Product

<!-- impeccable:product-schema 1 -->

## Platform

desktop

Native desktop app for macOS, Windows and Linux, written in Rust on GPUI (no
web view). Keyboard shortcuts and window chrome follow desktop conventions;
everything else follows the SigVault design system shared with the web
dashboard (n1rna/sigvault `apps/dashboard`).

## Users

People who hold bitcoin with hardware signing devices, alone or as part of a
group (co-founders, families, business partners, escrow parties, DAO
delegates). The same person often uses both workspaces:

- **On this device:** someone keeping their own wallets and keys on their own
  computer, with no account: a key library (generated keys, imported recovery
  phrases or xpubs, connected hardware wallets) combined into single-key,
  M-of-N multisig or time-locked vaults, then receive, send and restore.
- **SigVault Cloud:** a signer in a team's vault on sigvault.org who joins
  device-registration and signing sessions from their desk, with the hardware
  wallet plugged in.

Both workspaces are first-class; neither is the add-on.

## Product Purpose

Let a person sign and manage bitcoin with hardware wallets from their own
computer, either fully locally or as one signer in a SigVault vault. Success
means they always know what they are signing, which device and key is
involved, and who else still has to act.

## Positioning

Conductor, not custodian. Keys on users' devices never leave them; the app
talks to the hardware wallet on the desk and SigVault only relays signatures.
The local workspace works with no account at all; it syncs through SigVault's
Esplora servers (regtest, testnet4) or the user's own Esplora or Electrum server. The app is open source and builds reproducibly from
n1rna/sigvault-desktop.

## Operating Context

- Workspaces switch from the top bar or with ⌘1 / ⌘2 (Ctrl on Windows/Linux).
- Hardware: Ledger, BitBox02, Jade, Trezor, Coldcard and others via HWI;
  PSBT exchange by file and animated QR for air-gapped devices.
- Networks: regtest and testnet (testnet4) for SigVault Cloud today; local
  wallets choose their own network and server (Esplora `https://…`, or Electrum
  `ssl://` / `tcp://`).
- Updates ship through the built-in updater, signed with the release minisign
  key.

## Capabilities and Constraints

- Wallets stay in Liana format; same-timelock recovery paths are refused.
- Private keys in the local workspace are encrypted at rest under the
  keychain password.
- GPUI draws straight to the GPU: there is no DOM, so web tooling (browser
  detectors, DOM screenshots) does not apply. Debug builds expose an e2e
  driver (`SIGVAULT_E2E_DRIVER`) and screenshot fixtures
  (`SIGVAULT_DEV_CLOUD_FIXTURE`, `SIGVAULT_DEV_PAGE`, `SIGVAULT_DEV_WIZARD`).
- Light and dark themes, following the operating system by default with a
  manual override.

## Brand Commitments

- Name: SigVault (SigVault Desktop for the app).
- Same visual system as the web dashboard: Manrope for UI and labels, Space
  Mono only for real data (addresses, amounts, fingerprints, xpubs,
  descriptors, block heights), WCAG AA contrast in both themes, the deeper
  brand fill with white text on primary buttons.
- Voice: plain, precise, honest about limits.

## Product Principles

1. **Say plainly who can move the funds.** Keys on the user's devices stay
   there; where a SigVault-held key can spend on its own, the UI says so.
2. **Show what is being signed.** Amounts, destinations, policy and the device
   in use are visible at the moment of signing, never behind a click.
3. **Ceremonies involve several people.** Make clear who has signed, who is
   still needed, and what happens next.
4. **One system across web and desktop.** A person moving between the
   dashboard and the app should not have to relearn anything.

## Accessibility & Inclusion

WCAG 2.1 AA contrast in both themes; full keyboard operation of every flow;
no information conveyed by colour alone.
