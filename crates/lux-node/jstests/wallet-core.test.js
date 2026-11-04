// SPDX-License-Identifier: GPL-2.0-or-later
// Run: node --test crates/lux-node/jstests/
const test = require("node:test");
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
require("../static/wordlists.js");
const W = require("../static/wallet-core.js");

const unhex = W.unhex;
const SPKI = Buffer.from("302a300506032b6570032100", "hex");
function verify(pubHex, message, sigHex) {
  const key = crypto.createPublicKey({ key: Buffer.concat([SPKI, Buffer.from(pubHex, "hex")]), format: "der", type: "spki" });
  return crypto.verify(null, Buffer.from(message), key, Buffer.from(sigHex, "hex"));
}

test("official BIP-39 vectors (english list)", async () => {
  const v = [
    ["00000000000000000000000000000000", "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"],
    ["7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f", "legal winner thank year wave sausage worth useful legal winner thank yellow"],
    ["80808080808080808080808080808080", "letter advice cage absurd amount doctor acoustic avoid letter advice cage above"],
    ["ffffffffffffffffffffffffffffffff", "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong"],
  ];
  for (const [ent, phrase] of v) {
    assert.equal((await W.entropyToWords(unhex(ent), "en")).join(" "), phrase);
    const back = await W.wordsToEntropy(phrase);
    assert.equal(W.hex(back.entropy), ent);
    assert.equal(back.lang, "en");
  }
});

test("the same secret can be written in any list and read back", async () => {
  for (let i = 0; i < 25; i++) {
    const e = W.generateEntropy();
    for (const lang of W.LANGS) {
      const words = await W.entropyToWords(e, lang);
      assert.equal(words.length, 12);
      const back = await W.wordsToEntropy(words.join(" "));
      assert.equal(W.hex(back.entropy), W.hex(e));
    }
  }
});

test("typing: any case, extra spaces, no accents (spanish)", async () => {
  const e = unhex("0123456789abcdef0123456789abcdef");
  const es = await W.entropyToWords(e, "es");
  const typed = "  " + es.map((w) => w.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toUpperCase()).join("   ") + " ";
  assert.equal(W.hex((await W.wordsToEntropy(typed)).entropy), W.hex(e));
});

test("errors are caught: wrong count, unknown word, bad checksum, swapped words", async () => {
  await assert.rejects(W.wordsToEntropy("abandon abandon"), /12 parole/);
  await assert.rejects(W.wordsToEntropy("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon zzzzzz"), /lista/);
  await assert.rejects(W.wordsToEntropy("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon"), /errore/);
  const words = await W.entropyToWords(unhex("00112233445566778899aabbccddeeff"), "en");
  const swapped = words.slice(); [swapped[0], swapped[1]] = [swapped[1], swapped[0]];
  await assert.rejects(W.wordsToEntropy(swapped), /errore/);
});

test("the key is deterministic, depends on the network, and signs", async () => {
  const e = unhex("00112233445566778899aabbccddeeff");
  const a = await W.deriveKeys(e, "net-A");
  const a2 = await W.deriveKeys(e, "net-A");
  const b = await W.deriveKeys(e, "net-B");
  assert.equal(a.publicKeyHex, a2.publicKeyHex);
  assert.notEqual(a.publicKeyHex, b.publicKeyHex);
  assert.equal(a.publicKeyHex.length, 64);
  const msg = W.registrationMessage("net-A", "fulano", a.publicKeyHex);
  const sig = await W.sign(a.privateKey, msg);
  assert.equal(sig.length, 128);
  assert.ok(verify(a.publicKeyHex, msg, sig));
  assert.ok(!verify(a.publicKeyHex, msg + "x", sig));
  assert.ok(!verify(b.publicKeyHex, msg, sig));
  // the private key cannot be read back
  assert.equal(a.privateKey.extractable, false);
  await assert.rejects(crypto.subtle.exportKey("pkcs8", a.privateKey));
});

test("amounts and payload", () => {
  assert.equal(W.parseAmount("12"), 1200);
  assert.equal(W.parseAmount("12,5"), 1250);
  assert.equal(W.parseAmount("0.05"), 5);
  assert.equal(W.parseAmount("0"), null);
  assert.equal(W.parseAmount("1.234"), null);
  assert.equal(W.parseAmount("-3"), null);
  assert.equal(W.formatAmount(5), "0.05");
  assert.equal(W.formatAmount(123456), "1234.56");
  const p = W.payPayload({ net: "n", from: "a", to: "b", amountCenti: 1250, nonce: 3, memo: "ciao", t: 1700000000 });
  assert.equal(p, '{"v":1,"net":"n","kind":"pay","from":"a","to":"b","amount":1250,"nonce":3,"memo":"ciao","t":1700000000}');
});

test("writes the vectors that the Rust tests check", async () => {
  const entropy = "00112233445566778899aabbccddeeff", net = "0123456789abcdef0123456789abcdef";
  const k = await W.deriveKeys(unhex(entropy), net);
  const regMsg = W.registrationMessage(net, "fulano", k.publicKeyHex);
  const payload = W.payPayload({ net, from: "fulano", to: "mengano", amountCenti: 1250, nonce: 1, memo: "pane", t: 1700000000 });
  const vectors = {
    entropy, network_id: net, public_key: k.publicKeyHex,
    words_en: (await W.entropyToWords(unhex(entropy), "en")).join(" "),
    words_it: (await W.entropyToWords(unhex(entropy), "it")).join(" "),
    registration: { username: "fulano", message: regMsg, signature: await W.sign(k.privateKey, regMsg) },
    pay: { payload, signature: await W.sign(k.privateKey, payload) },
  };
  fs.mkdirSync(path.join(__dirname, "vectors"), { recursive: true });
  fs.writeFileSync(path.join(__dirname, "vectors", "wallet-vectors.json"), JSON.stringify(vectors, null, 2) + "\n");
  assert.ok(verify(vectors.public_key, regMsg, vectors.registration.signature));
});
