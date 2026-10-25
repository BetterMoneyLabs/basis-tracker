#!/usr/bin/env python3
"""Generate an ephemeral secp256k1 keypair for a demo run.

Prints "<pubkey_hex> <secret_hex>" on one line.

The demo scripts used to carry hardcoded keypairs in the repository (and, for
`agent_celaut_use`, also in a committed `config/basis.toml`). Because those values were public they
had to be treated as compromised, so each run now mints a fresh keypair that lives only in the
gitignored generated config.

No third-party dependencies: secp256k1 scalar multiplication over the short Weierstrass curve
y^2 = x^3 + 7 in F_p, with a constant-time-enough double-and-add ladder. This is a demo helper for
generating throwaway keys, not a production key generator -- use a real wallet for real funds.
"""

import secrets
import sys

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8
G = (GX, GY)


def inv_mod(a: int, m: int) -> int:
    return pow(a, m - 2, m)


def point_add(p1, p2):
    """Add two points, handling the identity element (None) and doubling."""
    if p1 is None:
        return p2
    if p2 is None:
        return p1
    x1, y1 = p1
    x2, y2 = p2
    if x1 == x2:
        if (y1 + y2) % P == 0:
            return None  # P + (-P) = identity
        lam = (3 * x1 * x1) * inv_mod(2 * y1 % P, P) % P
    else:
        lam = (y2 - y1) * inv_mod((x2 - x1) % P, P) % P
    x3 = (lam * lam - x1 - x2) % P
    y3 = (lam * (x1 - x3) - y1) % P
    return (x3, y3)


def point_mul(k: int, point=G):
    """Scalar multiplication via double-and-add."""
    result = None
    addend = point
    while k:
        if k & 1:
            result = point_add(result, addend)
        addend = point_add(addend, addend)
        k >>= 1
    return result


def compressed(point) -> str:
    x, y = point
    return ("%02x" % (2 + (y & 1))) + ("%064x" % x)


def generate_keypair() -> tuple[str, str]:
    while True:
        secret = secrets.randbelow(N - 1) + 1
        point = point_mul(secret)
        if point is not None:
            return compressed(point), "%064x" % secret


if __name__ == "__main__":
    pub, sec = generate_keypair()
    print(f"{pub} {sec}", file=sys.stdout)
