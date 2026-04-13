// Key derivation utilities.
//
// Two paths:
//   * `derive_password_key` — Argon2id, for user-provided passwords (Stronghold).
//   * `derive_machine_key`  — SHA-256 of a machine-bound secret, for internal
//                             at-rest encryption where no user password exists.
//
// Both functions mix in a machine identifier so that encrypted state copied
// to a different machine cannot be decrypted even if the attacker knows the
// password / has the ciphertext.

use argon2::{Algorithm, Argon2, Params, Version};
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"sigvault-desktop-v1";

fn machine_id_bytes() -> Vec<u8> {
    machine_uid::get()
        .unwrap_or_else(|_| "unknown-machine".to_string())
        .into_bytes()
}

/// 16-byte salt derived from the machine identifier. Stable across runs on
/// the same machine, different across machines.
fn machine_salt() -> [u8; 16] {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(b"|salt|");
    h.update(machine_id_bytes());
    let digest = h.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

/// Derive a 32-byte key from a user password using Argon2id with a
/// machine-bound salt. Use this for interactive secrets (e.g. Stronghold).
pub fn derive_password_key(password: &[u8]) -> [u8; 32] {
    let salt = machine_salt();
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default());
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(password, &salt, &mut out)
        .expect("argon2 derivation failed");
    out
}

/// Derive a 32-byte key bound to this machine for a given purpose label.
/// Not password-protected — the security property is "cannot be decrypted
/// without also having access to this machine's identifier".
pub fn derive_machine_key(purpose: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update(b"|key|");
    h.update(purpose);
    h.update(b"|");
    h.update(machine_id_bytes());
    let digest = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_key_is_deterministic_on_same_machine() {
        let a = derive_password_key(b"hunter2");
        let b = derive_password_key(b"hunter2");
        assert_eq!(a, b);
    }

    #[test]
    fn different_passwords_give_different_keys() {
        let a = derive_password_key(b"hunter2");
        let b = derive_password_key(b"hunter3");
        assert_ne!(a, b);
    }

    #[test]
    fn machine_key_differs_by_purpose() {
        let a = derive_machine_key(b"purpose-a");
        let b = derive_machine_key(b"purpose-b");
        assert_ne!(a, b);
    }
}
