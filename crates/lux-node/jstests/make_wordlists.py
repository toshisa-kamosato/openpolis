#!/usr/bin/env python3
"""Builds static/wordlists.js from the three official BIP-39 lists (static/bip39/*.txt)."""
import json, pathlib
root = pathlib.Path(__file__).resolve().parent.parent
langs = {"it": "italian", "en": "english", "es": "spanish"}
data = {}
for code, name in langs.items():
    words = (root / "static" / "bip39" / f"{name}.txt").read_text(encoding="utf-8").split("\n")
    words = [w.strip() for w in words if w.strip()]
    assert len(words) == 2048 and len(set(words)) == 2048, name
    data[code] = words
out = "// SPDX-License-Identifier: MIT\n// BIP-39 word lists (italian, english, spanish) from github.com/bitcoin/bips/bip-0039. Generated: do not edit.\n"
out += "(function (root) { root.OPENPOLIS_WORDLISTS = " + json.dumps(data, ensure_ascii=False, separators=(",", ":")) + "; })(typeof window !== \"undefined\" ? window : globalThis);\n"
(root / "static" / "wordlists.js").write_text(out, encoding="utf-8")
print("wordlists.js:", len(out), "byte")
