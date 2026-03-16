# Trezor Taproot Signing Limitations

## Summary

The Trezor firmware (as of v2.10.0) only supports **keypath spending for simple taproot outputs** (`tr(key)`) without script trees. It **cannot** produce valid signatures for taproot outputs with script trees (`tr(key, tree)`).

## Root Cause

The Trezor firmware's `bip340_sign` function applies the BIP-340 key tweak using only the internal key:

```python
output_private_key = bip340.tweak_secret_key(internal_private_key)
```

This computes `tweak = H(internal_key)` (the "nothing up my sleeve" tweak). For outputs with a script tree, the correct tweak is `H(internal_key || merkle_root)`, but the firmware does not pass the merkle root to the tweak function.

Additionally, the `trezor-client` Rust crate (v0.1.5) has no protocol field to send the merkle root or tap tree data to the device.

## What Works

- **Simple taproot keypath**: `tr(key)` — Trezor as the sole keypath signer, no script tree
- **Discovery and xpub extraction**: Works for all taproot derivation paths
- **Segwit v0**: `wpkh(key)`, `wsh(...)` — fully supported

## What Does NOT Work

- **Taproot with script tree (keypath)**: `tr(key, script_tree)` — Trezor cannot sign keypath because the tweak is wrong
- **Taproot script-path spending**: The Trezor protocol does not support script-path signing at all
- **Multi-party taproot**: Wallets where Trezor is one of multiple signers in a taproot tree

## Implications for SigVault

When creating a wallet with spending conditions (timelocks, multisig recovery), the resulting descriptor has a script tree. If a Trezor device is assigned as the keypath signer in such a wallet, it will be unable to sign transactions.

**Validation rule**: If a wallet has spending conditions (script tree), do not allow a Trezor device as the keypath (primary) signer. Trezor devices can only be used in simple single-key taproot wallets without additional spending conditions.

## References

- [Trezor firmware bip340_sign](https://github.com/trezor/trezor-firmware/blob/main/core/src/apps/bitcoin/common.py)
- [Taproot in Trezor T PR #1891](https://github.com/trezor/trezor-firmware/pull/1891)
- [HWI Taproot signature issues #591](https://github.com/bitcoin-core/HWI/issues/591)
