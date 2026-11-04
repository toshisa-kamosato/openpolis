// SPDX-License-Identifier: GPL-2.0-or-later
// The real thing, without a browser: a real `luxd` process, the real wallet-core.js signing in Node, real HTTP.
const test = require("node:test");
const assert = require("node:assert/strict");
require("../static/wordlists.js");
const W = require("../static/wallet-core.js");
const { startNode, makeMembers, PW } = require("./support.js");

async function createWallet(client, username) {
  const net = (await client.call("GET", "/api/network")).json;
  const entropy = W.generateEntropy();
  const words = await W.entropyToWords(entropy, "it");
  const k = await W.deriveKeys(entropy, net.network_id);
  const proof = await W.sign(k.privateKey, W.registrationMessage(net.network_id, username, k.publicKeyHex));
  const r = await client.call("POST", "/api/me/key", { public_key: k.publicKeyHex, signature: proof, password: PW });
  assert.equal(r.status, 200, r.text);
  return { words, key: k.privateKey, pub: k.publicKeyHex, net: net.network_id, next: 1 };
}
async function pay(client, who, from, to, amountCenti, memo = "") {
  const payload = W.payPayload({ net: who.net, from, to, amountCenti, nonce: who.next, memo, t: Math.floor(Date.now() / 1000) });
  const r = await client.call("POST", "/api/pay", { payload, signature: await W.sign(who.key, payload) });
  if (r.status === 200) who.next++;
  return r;
}

test("two members, wallets made in the browser, a signed payment, restart, verification", async () => {
  const node = await startNode();
  try {
    const { members } = await makeMembers(node.base, ["fulano", "mengano"]);
    const [a, b] = [members.fulano, members.mengano];
    const wa = await createWallet(a, "fulano");
    const wb = await createWallet(b, "mengano");
    let w = (await a.call("GET", "/api/wallet")).json;
    assert.equal(w.public_key, wa.pub);
    assert.equal(w.balance, 0);

    const r = await pay(a, wa, "fulano", "mengano", 1000, "pane");
    assert.equal(r.status, 200, r.text);
    assert.deepEqual([r.json.cost, r.json.seller_receives, r.json.borrowed], [1025, 975, 1025]);
    w = (await a.call("GET", "/api/wallet")).json;
    assert.deepEqual([w.balance, w.debt, w.fido, w.credit_available, w.next_nonce], [0, 1025, 10000, 8975, 2]);
    const wb1 = (await b.call("GET", "/api/wallet")).json;
    assert.equal(wb1.balance, 975);
    assert.equal(wb1.recent[0].memo, "pane");

    // the same signed payment again is refused, and a payment signed with the wrong key too
    const payload = W.payPayload({ net: wa.net, from: "fulano", to: "mengano", amount: 1, amountCenti: 1000, nonce: 1, memo: "pane", t: 1 });
    assert.equal((await a.call("POST", "/api/pay", { payload, signature: await W.sign(wa.key, payload) })).status, 409);
    const forged = W.payPayload({ net: wa.net, from: "fulano", to: "mengano", amountCenti: 500, nonce: wa.next, memo: "", t: 1 });
    assert.equal((await a.call("POST", "/api/pay", { payload: forged, signature: await W.sign(wb.key, forged) })).status, 400, "signed by mengano's key");
    // mengano cannot spend in fulano's name
    const stolen = W.payPayload({ net: wa.net, from: "fulano", to: "mengano", amountCenti: 500, nonce: wa.next, memo: "", t: 1 });
    assert.equal((await b.call("POST", "/api/pay", { payload: stolen, signature: await W.sign(wb.key, stolen) })).status, 400);

    // the phrase restores the same key, in any language
    const back = await W.wordsToEntropy(wa.words.join(" "));
    const again = await W.deriveKeys(back.entropy, wa.net);
    assert.equal(again.publicKeyHex, wa.pub);
    const es = await W.entropyToWords(back.entropy, "es");
    assert.equal((await W.deriveKeys((await W.wordsToEntropy(es)).entropy, wa.net)).publicKeyHex, wa.pub);

    // restart: the economy is rebuilt from the ledger
    await node.stop();
    const v = node.luxd(["verify"]);
    assert.equal(v.status, 0, v.stderr);
    assert.match(v.stdout, /Registro valido: 7 voci/);
    assert.match(node.luxd(["status"]).stdout, /3 membri/);
  } finally { await node.stop().catch(() => {}); }
});
