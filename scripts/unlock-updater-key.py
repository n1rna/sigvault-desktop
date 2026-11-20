#!/usr/bin/env python3
"""Print the updater signing key with its password encryption removed.

The key was made by `tauri signer generate`, which scrypt-encrypts it even
when the password is empty. cargo-packager 0.11 reads keys through minisign
0.7.9, which takes an empty password to mean "not encrypted" and skips the
KDF, so it rejects this key ("Wrong password for that key") no matter what
we pass. This does the decryption minisign used to do and prints the key in
the same format (base64 of a minisign secret key file), unencrypted, for
cargo-packager to use with an empty password.

    key (stdin), password ($KEY_PASSWORD, may be empty) -> key (stdout)

A key that is already unencrypted passes through unchanged. Exits non-zero
if the checksum doesn't match, i.e. the password is wrong.
Standard library only, so it runs on every release runner.
"""

import base64
import hashlib
import os
import sys

TWO = 2
SALT = 32
U64 = 8
KEYNUM = 8
SECRET = 64
CHK = 32


def scrypt_params(memlimit: int, opslimit: int) -> tuple[int, int, int]:
    # libsodium's pickparams, as minisign's raw_scrypt_params.
    opslimit = max(32768, opslimit)
    r = 8
    if opslimit < memlimit // 32:
        p = 1
        maxn = opslimit // (r * 4)
    else:
        maxn = memlimit // (r * 128)
    n_log2 = 1
    while n_log2 < 63 and not (1 << n_log2) > maxn // 2:
        n_log2 += 1
    if opslimit >= memlimit // 32:
        p = min(0x3FFFFFFF, (opslimit // 4) // (1 << n_log2)) // r
    if n_log2 > 20:
        sys.exit("scrypt parameters too high")
    return 1 << n_log2, r, p


def checksum(sig_alg: bytes, keynum: bytes, sk: bytes) -> bytes:
    return hashlib.blake2b(sig_alg + keynum + sk, digest_size=CHK).digest()


def main() -> None:
    wrapped = sys.stdin.read().strip()
    password = os.environ.get("KEY_PASSWORD", "")

    lines = base64.b64decode(wrapped).decode().splitlines()
    if len(lines) < 2:
        sys.exit("not a minisign secret key")
    comment, raw = lines[0], bytearray(base64.b64decode(lines[1]))

    o = 0
    sig_alg = bytes(raw[o : o + TWO]); o += TWO
    kdf_at = o; o += TWO
    o += TWO  # chk_alg
    salt = bytes(raw[o : o + SALT]); o += SALT
    opslimit = int.from_bytes(raw[o : o + U64], "little"); o += U64
    memlimit = int.from_bytes(raw[o : o + U64], "little"); o += U64
    secret_at = o
    secret_len = KEYNUM + SECRET + CHK
    if len(raw) != secret_at + secret_len:
        sys.exit("unexpected secret key length")

    def parts(buf: bytes) -> tuple[bytes, bytes, bytes]:
        return buf[:KEYNUM], buf[KEYNUM : KEYNUM + SECRET], buf[KEYNUM + SECRET :]

    secret = bytes(raw[secret_at:])
    keynum, sk, chk = parts(secret)
    if checksum(sig_alg, keynum, sk) != chk:
        n, r, p = scrypt_params(memlimit, opslimit)
        stream = hashlib.scrypt(
            password.encode(), salt=salt, n=n, r=r, p=p, maxmem=1 << 30, dklen=secret_len
        )
        secret = bytes(a ^ b for a, b in zip(secret, stream))
        keynum, sk, chk = parts(secret)
        if checksum(sig_alg, keynum, sk) != chk:
            sys.exit("wrong password for the updater signing key")

    raw[kdf_at : kdf_at + TWO] = b"\0\0"
    raw[secret_at:] = secret
    out = f"{comment}\n{base64.b64encode(bytes(raw)).decode()}\n"
    sys.stdout.write(base64.b64encode(out.encode()).decode())


if __name__ == "__main__":
    main()
