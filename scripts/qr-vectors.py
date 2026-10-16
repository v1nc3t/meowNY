#!/usr/bin/env python3
"""Reference QR matrices for the encoder in ui/src/lib/qr.ts (written to ui/src/lib/qr.vectors.json).

The encoder is written out in TypeScript, with no library, and its tests compare it module for module with the matrices that
`segno` makes for the same text, error level, version and mask. `segno` is needed only to regenerate this file:

    python3 -m venv /tmp/qr && /tmp/qr/bin/pip install segno && /tmp/qr/bin/python scripts/qr-vectors.py

Every version from 1 to 10 at every error level is covered at its fullest and at about half, and masks 0 to 7 are forced on
three versions (one of them with version information), so a table or a rule that is wrong anywhere shows up as a difference.
"""
import json
import os

import segno
from segno import encoder


def _padding_bits(buff, version, length):
    # ISO/IEC 18004, 7.4.10: zeros are added only up to the next codeword boundary. segno adds `8 - (length % 8)`, which is a whole
    # extra zero byte when the data already ends on a boundary (always, in byte mode, after the terminator). Scanners ignore that
    # byte, but it is not what the standard says, so the reference is corrected here.
    if version > 0:  # not Micro QR (versions M1 to M4 are below 1 in segno)
        buff.extend([0] * (-length % 8))


encoder.write_padding_bits = _padding_bits

LEVELS = "LMQH"


def text_of(n, seed):
    """Printable ASCII with lower case in it, so the data is always byte mode."""
    state, out = seed, []
    for _ in range(n):
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        out.append(chr(33 + (state >> 8) % 94))
    out[0] = "a"
    return "".join(out)


def capacity(version, level):
    n = 1
    while True:
        try:
            segno.make("a" * (n + 1), version=version, error=level, mode="byte", boost_error=False, micro=False)
            n += 1
        except Exception:
            return n


def rows_of(code):
    width = (len(code.matrix) + 3) // 4
    out = []
    for row in code.matrix:
        bits = "".join("1" if m else "0" for m in row)
        out.append(format(int(bits, 2), "0%dx" % width))
    return out


def case(text, level, version, mask=None):
    code = segno.make(text, error=level, version=version, mask=mask, mode="byte", boost_error=False, micro=False)
    return {
        "text": text,
        "level": level,
        "version": code.version if version is None else version,
        "forcedVersion": version is not None,
        "mask": code.mask,
        "forcedMask": mask is not None,
        "size": len(code.matrix),
        "rows": rows_of(code),
    }


def main():
    cases = []
    seed = 1
    for version in range(1, 11):
        for level in LEVELS:
            full = capacity(version, level)
            for n in (full, full // 2 + 1):
                seed += 1
                cases.append(case(text_of(n, seed), level, version))
    # forced masks: version 2 (no version information), 5 and 8 (version 8 carries version information)
    for version, level in ((2, "L"), (5, "M"), (8, "Q")):
        n = capacity(version, level) - 3
        for mask in range(8):
            cases.append(case(text_of(n, 100 + mask), level, version, mask))
    # the smallest version that fits, as the app asks for it
    for n in (1, 10, 30, 60, 72, 100, 156):
        for level in "LM":
            cases.append(case(text_of(n, 500 + n), level, None))
    # the codes the app really makes
    for text in (
        "MNY1:AQERIjNEVWZ3iEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9gwAABBMCoARQ",
        "MNY1:AQIRIjNEVWZ3iAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gw1ECBAoAAAUG_QAAAAAAAAAAAAAAAAAAAQ",
    ):
        for level in "LMQ":
            cases.append(case(text, level, None))
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ui", "src", "lib", "qr.vectors.json")
    with open(path, "w") as out:
        json.dump({"segno": segno.__version__, "cases": cases}, out, separators=(",", ":"))
        out.write("\n")
    print("wrote", len(cases), "cases to", os.path.normpath(path))


if __name__ == "__main__":
    main()
