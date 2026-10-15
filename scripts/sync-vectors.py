#!/usr/bin/env python3
"""A second, independent implementation of the sync handshake and frames (docs/sync-design.md, sections 6 and 16).

It uses nothing but the Python standard library: HKDF-SHA256 from `hmac` (RFC 5869) and ChaCha20-Poly1305 written out
from RFC 8439. Before it prints anything it checks its own ChaCha20-Poly1305 against the test vector in RFC 8439,
section 2.8.2. The Rust code (src-tauri/src/sync/crypto.rs) has the printed values in its tests, so the two programs
are made to agree. A reviewer can read this file next to the design and run it:

    python3 scripts/sync-vectors.py            # prints the vectors
    python3 scripts/sync-vectors.py --rust     # prints them as the Rust constants the tests hold
"""
import base64
import hashlib
import hmac
import struct
import sys

MASK = 0xFFFFFFFF


def rotl(v, n):
    return ((v << n) & MASK) | (v >> (32 - n))


def quarter(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & MASK; s[d] = rotl(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & MASK; s[b] = rotl(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & MASK; s[d] = rotl(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & MASK; s[b] = rotl(s[b] ^ s[c], 7)


def chacha_block(key, counter, nonce):
    const = [0x61707865, 0x3320646E, 0x79622D32, 0x6B206574]
    state = const + list(struct.unpack("<8I", key)) + [counter] + list(struct.unpack("<3I", nonce))
    work = state[:]
    for _ in range(10):
        quarter(work, 0, 4, 8, 12); quarter(work, 1, 5, 9, 13); quarter(work, 2, 6, 10, 14); quarter(work, 3, 7, 11, 15)
        quarter(work, 0, 5, 10, 15); quarter(work, 1, 6, 11, 12); quarter(work, 2, 7, 8, 13); quarter(work, 3, 4, 9, 14)
    return struct.pack("<16I", *[(w + s) & MASK for w, s in zip(work, state)])


def chacha_xor(key, counter, nonce, data):
    out = bytearray()
    for i in range(0, len(data), 64):
        block = chacha_block(key, counter + i // 64, nonce)
        out += bytes(a ^ b for a, b in zip(data[i:i + 64], block))
    return bytes(out)


def poly1305(key, msg):
    r = int.from_bytes(key[:16], "little") & 0x0FFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF
    s = int.from_bytes(key[16:], "little")
    p = (1 << 130) - 5
    acc = 0
    for i in range(0, len(msg), 16):
        chunk = msg[i:i + 16]
        acc = ((acc + int.from_bytes(chunk + b"\x01", "little")) * r) % p
    return ((acc + s) & ((1 << 128) - 1)).to_bytes(16, "little")


def pad16(b):
    return b"\x00" * ((16 - len(b) % 16) % 16)


def aead_seal(key, nonce, aad, plaintext):
    otk = chacha_block(key, 0, nonce)[:32]
    ct = chacha_xor(key, 1, nonce, plaintext)
    mac_data = aad + pad16(aad) + ct + pad16(ct) + struct.pack("<QQ", len(aad), len(ct))
    return ct + poly1305(otk, mac_data)


def hkdf(salt, ikm, info, length=32):
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    out, block, i = b"", b"", 1
    while len(out) < length:
        block = hmac.new(prk, block + info + bytes([i]), hashlib.sha256).digest()
        out += block
        i += 1
    return out[:length]


def self_test():
    # RFC 8439, section 2.8.2
    key = bytes(range(0x80, 0xA0))
    nonce = bytes.fromhex("070000004041424344454647")
    aad = bytes.fromhex("50515253c0c1c2c3c4c5c6c7")
    text = (b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, "
            b"sunscreen would be it.")
    sealed = aead_seal(key, nonce, aad, text)
    assert sealed[-16:].hex() == "1ae10b594f09e26a7e902ecbd0600691", "ChaCha20-Poly1305 does not match RFC 8439"
    assert sealed[:16].hex() == "d31a8d34648e60db7b86afbc53ef7ec2"
    # RFC 5869, test case 1
    okm = hkdf(bytes.fromhex("000102030405060708090a0b0c"), bytes.fromhex("0b" * 22), bytes.fromhex("f0f1f2f3f4f5f6f7f8f9"), 42)
    assert okm.hex() == "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865", "HKDF"


MAGIC = b"MNY1"


def handshake(kind, ps, ots, session_id, np, nd):
    ikm = ps + ots if kind == 1 else ps
    header = MAGIC + bytes([kind]) + session_id + np
    k1 = hkdf(MAGIC + bytes([kind]) + session_id + np, ikm, b"k1")
    m1 = header + aead_seal(k1, bytes(12), header, b"hello")
    salt = MAGIC + bytes([kind]) + session_id + np + nd
    k2 = hkdf(salt, ikm, b"k2")
    m2 = nd + aead_seal(k2, bytes(12), m1 + nd, b"reply")
    p2d, d2p = hkdf(salt, ikm, b"p2d"), hkdf(salt, ikm, b"d2p")
    check = int.from_bytes(hkdf(salt, ikm, b"check", 4), "big") % 1_000_000
    return dict(m1=m1, m2=m2, k1=k1, k2=k2, p2d=p2d, d2p=d2p, check="%06d" % check)


def frame(key, direction, counter, plaintext):
    nonce = bytes(4) + counter.to_bytes(8, "big")
    return aead_seal(key, nonce, bytes([direction]) + counter.to_bytes(8, "big"), plaintext)


def qr_text(kind, session_id, secret, port, addrs):
    """The text of a QR code (design section 4): `MNY1:` and the bytes in base64url without padding."""
    body = bytes([1, kind]) + session_id + secret + port.to_bytes(2, "big") + bytes([len(addrs)])
    for address in addrs:
        body += bytes([4 if len(address) == 4 else 6]) + address
    return "MNY1:" + base64.urlsafe_b64encode(body).rstrip(b"=").decode()


def vectors():
    ps = bytes(range(0x01, 0x21))
    ots = bytes(range(0x41, 0x61))
    session_id = bytes.fromhex("1122334455667788")
    np, nd = bytes(range(0xA0, 0xC0)), bytes(range(0xB0, 0xD0))
    out = {"ps": ps, "ots": ots, "session_id": session_id, "np": np, "nd": nd}
    for name, kind in (("sync", 1), ("pair", 2)):
        h = handshake(kind, ps, ots, session_id, np, nd)
        out[name] = h
    h = out["sync"]
    out["frame_p2d_0"] = frame(h["p2d"], 1, 0, b"hello desktop")
    out["frame_p2d_1"] = frame(h["p2d"], 1, 1, b"second")
    out["frame_d2p_0"] = frame(h["d2p"], 2, 0, b"hello phone")
    out["qr_sync"] = qr_text(1, session_id, ots, 49152, [bytes([192, 168, 1, 20])]).encode()
    out["qr_pair"] = qr_text(2, session_id, ps, 50001, [bytes([10, 0, 0, 5]), bytes.fromhex("fd000000000000000000000000000001")]).encode()
    return out


if __name__ == "__main__":
    self_test()
    v = vectors()
    rust = "--rust" in sys.argv
    lines = []

    def emit(name, value):
        if rust:
            lines.append(f'const {name.upper()}: &str = "{value}";')
        else:
            lines.append(f"{name}: {value}")

    for key in ("ps", "ots", "session_id", "np", "nd"):
        emit(key, v[key].hex())
    for mode in ("sync", "pair"):
        for key in ("m1", "m2", "k1", "k2", "p2d", "d2p"):
            emit(f"{mode}_{key}", v[mode][key].hex())
        emit(f"{mode}_check", v[mode]["check"])
    for key in ("frame_p2d_0", "frame_p2d_1", "frame_d2p_0"):
        emit(key, v[key].hex())
    for key in ("qr_sync", "qr_pair"):
        emit(key, v[key].decode())
    print("\n".join(lines))
