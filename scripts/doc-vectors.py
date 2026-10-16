#!/usr/bin/env python3
"""A second, independent implementation of the sealed document file (docs/documents-design.md, sections 3, 4 and 17).

It uses nothing but the Python standard library. The ChaCha20-Poly1305 and HKDF-SHA256 are the ones of
`scripts/sync-vectors.py`, written out from RFC 8439 and RFC 5869, which check themselves against the RFCs' own test vectors
before anything is printed. The Rust code (src-tauri/src/documents/seal.rs) has the printed values in its tests, so the two
programs are made to agree. A reviewer can read this file next to the design and run it:

    python3 scripts/doc-vectors.py            # prints the vectors
    python3 scripts/doc-vectors.py --rust     # prints them as the Rust constants the tests hold
"""
import hashlib
import hmac
import importlib.util
import os
import struct
import sys

sys.dont_write_bytecode = True  # importing the sync script must not leave a .pyc in scripts/
_here = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("sync_vectors", os.path.join(_here, "sync-vectors.py"))
sv = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(sv)

MAGIC = b"MEOWNYDF"
FORMAT = 1
CHUNK = 65536
TAG = 16
HEADER = 8 + 2 + 4 + 16


def hkdf_expand(prk, info, length):
    """HKDF-Expand alone (RFC 5869, section 2.3): the key is already uniformly random, so there is no extract step."""
    out, block, i = b"", b"", 1
    while len(out) < length:
        block = hmac.new(prk, block + info + bytes([i]), hashlib.sha256).digest()
        out += block
        i += 1
    return out[:length]


def docs_root(db_key):
    """No salt means a salt of 32 zero bytes (RFC 5869, section 2.2)."""
    return sv.hkdf(bytes(32), db_key, b"meowny documents v1")


def file_stem(root, doc_id):
    return hkdf_expand(root, b"meowny doc name v1" + doc_id, 16).hex()


def file_key(root, salt, doc_id):
    return sv.hkdf(salt, root, b"meowny doc file v1" + doc_id)


def header_of(salt):
    return MAGIC + struct.pack(">HI", FORMAT, CHUNK) + salt


def seal_file(key, salt, plain):
    """The header and the chunks. The nonce is a flag (1 on the last chunk), three zero bytes and the chunk number."""
    header = header_of(salt)
    n = max(1, -(-len(plain) // CHUNK))
    out = header
    for i in range(n):
        piece = plain[i * CHUNK:(i + 1) * CHUNK]
        last = 1 if i == n - 1 else 0
        nonce = bytes([last, 0, 0, 0]) + i.to_bytes(8, "big")
        out += sv.aead_seal(key, nonce, header, piece)
    return out


def pattern(n):
    """Byte i is i modulo 251, which never lines up with a chunk boundary."""
    return bytes(i % 251 for i in range(n))


# (name, contents). Small ones are printed whole; the large ones as a length and a SHA-256, since 65 KB of hex would not read.
CASES = [
    ("empty", b""),
    ("one_byte", b"A"),
    ("short", pattern(100)),
    ("chunk_less_one", pattern(CHUNK - 1)),
    ("one_chunk", pattern(CHUNK)),
    ("chunk_plus_one", pattern(CHUNK + 1)),
    ("two_chunks", pattern(2 * CHUNK)),
    ("two_and_a_bit", pattern(150000)),
]
WHOLE = {"empty", "one_byte", "short"}


def vectors():
    db_key = bytes(range(0x10, 0x30))
    doc_id = bytes(range(0xC0, 0xD0))
    salt = bytes(range(0x50, 0x60))
    root = docs_root(db_key)
    key = file_key(root, salt, doc_id)
    out = {
        "db_key": db_key,
        "doc_id": doc_id,
        "salt": salt,
        "root": root,
        "stem": file_stem(root, doc_id),
        "file_key": key,
        "header": header_of(salt),
    }
    for name, plain in CASES:
        sealed = seal_file(key, salt, plain)
        out[f"{name}_len"] = len(sealed)
        out[f"{name}_sha256"] = hashlib.sha256(sealed).hexdigest()
        if name in WHOLE:
            out[f"{name}_sealed"] = sealed
    return out


if __name__ == "__main__":
    sv.self_test()
    v = vectors()
    rust = "--rust" in sys.argv
    lines = []
    for key, value in v.items():
        text = value.hex() if isinstance(value, bytes) else str(value)
        if rust:
            if isinstance(value, int):
                lines.append(f"const {key.upper()}: usize = {text};")
            else:
                lines.append(f'const {key.upper()}: &str = "{text}";')
        else:
            lines.append(f"{key}: {text}")
    print("\n".join(lines))
