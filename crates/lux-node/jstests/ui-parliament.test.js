// SPDX-License-Identifier: GPL-2.0-or-later
// The parliament page (static/parliament.js) driven like a person would, against a real `luxd` process.
// jsdom + fake IndexedDB + Node's WebCrypto: it does NOT replace a try-out in real browsers.
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const nodeCrypto = require("node:crypto");
const { JSDOM } = require("jsdom");
const { IDBFactory } = require("fake-indexeddb");
const { startNode, makeMembers, PW } = require("./support.js");
require("../static/wordlists.js");
const W = require("../static/wallet-core.js");

const read = (f) => fs.readFileSync(path.join(__dirname, "../static", f), "utf8");

async function walletFor(client, username) {
  const net = (await client.call("GET", "/api/network")).json.network_id;
  const k = await W.deriveKeys(W.generateEntropy(), net);
  const proof = await W.sign(k.privateKey, W.registrationMessage(net, username, k.publicKeyHex));
  const r = await client.call("POST", "/api/me/key", { public_key: k.publicKeyHex, signature: proof, password: PW });
  assert.equal(r.status, 200, r.text);
  return { net, username, key: k.privateKey, pub: k.publicKeyHex, next: 1 };
}

function putKey(idb, who) {
  return new Promise((resolve, reject) => {
    const open = idb.open("openpolis", 1);
    open.onupgradeneeded = () => open.result.createObjectStore("keys");
    open.onerror = () => reject(open.error);
    open.onsuccess = () => {
      const tx = open.result.transaction("keys", "readwrite");
      tx.objectStore("keys").put({ publicKeyHex: who.pub, privateKey: who.key }, `${who.net}:${who.username}`);
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    };
  });
}

async function openPage(base, client, idb) {
  const html = (await client.call("GET", "/parliament")).text;
  const dom = new JSDOM(html, { url: base + "/parliament", runScripts: "outside-only", pretendToBeVisual: true });
  const w = dom.window;
  Object.defineProperty(w, "crypto", { value: nodeCrypto.webcrypto, configurable: true });
  w.TextEncoder = TextEncoder;
  w.indexedDB = idb;
  w.fetch = (url, opts = {}) => fetch(new URL(url, base), { ...opts, headers: { ...(opts.headers || {}), Cookie: client.header() } });
  for (const f of ["wordlists.js", "wallet-core.js", "parliament.js"]) w.eval(read(f));
  return dom;
}

async function until(fn, what, ms = 8000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (fn()) return; await new Promise((r) => setTimeout(r, 25)); }
  throw new Error("timeout waiting for: " + what);
}

async function apiVote(client, who, proposal, choice) {
  const payload = W.votePayload({ net: who.net, from: who.username, proposal, choice, nonce: who.next, t: Math.floor(Date.now() / 1000) });
  const r = await client.call("POST", "/api/vote", { payload, signature: await W.sign(who.key, payload) });
  if (r.status === 200) who.next++;
  return r;
}

test("propose, vote and change the vote from the parliament page", async () => {
  const node = await startNode();
  const doms = [];
  try {
    const { members } = await makeMembers(node.base, ["fulano", "mengano", "zutano"]);
    const wa = await walletFor(members.fulano, "fulano");
    const wb = await walletFor(members.mengano, "mengano");
    const wc = await walletFor(members.zutano, "zutano");

    // ---- a browser without the key: it can read, not act
    let dom = await openPage(node.base, members.fulano, new IDBFactory()); doms.push(dom);
    let d = dom.window.document;
    await until(() => d.getElementById("p-list").textContent.includes("nessuna proposta"), "empty list");
    assert.equal(d.getElementById("n-box").hidden, true);
    assert.equal(d.getElementById("n-nokey").hidden, false);

    // ---- the browser that has the key
    const idb = new IDBFactory();
    await putKey(idb, wa);
    dom = await openPage(node.base, members.fulano, idb); doms.push(dom);
    d = dom.window.document;
    const $ = (id) => d.getElementById(id);
    await until(() => !$("n-box").hidden, "proposal form");
    const articles = () => d.querySelectorAll("#p-list article");
    const text = () => d.getElementById("p-list").textContent;

    // a text proposal; the title is shown as text, never as HTML
    $("n-type").value = "text"; $("n-title").value = "<img src=x onerror=alert(1)> Piantiamo un albero"; $("n-body").value = "Davanti alla scuola.";
    $("n-go").click();
    await until(() => articles().length === 1, "first proposal");
    assert.ok(!d.querySelector("#p-list img"), "no HTML from the title");
    assert.match(text(), /In corso — Sì 0 · No 0 · Astenuti 0 — hanno votato 0 su 3 \(ne servono almeno 1\)/);

    // vote yes, then change to no
    const click = (label) => [...d.querySelectorAll("#p-list button")].find((b) => b.textContent === label).click();
    click("Sì");
    await until(() => /Sì 1 · No 0/.test(text()), "yes counted");
    assert.match(text(), /Il tuo voto: Sì/);
    click("No");
    await until(() => /Sì 0 · No 1/.test(text()), "vote changed");
    assert.match(text(), /Il tuo voto: No/);

    // an emission and a work show what they will do
    $("n-type").value = "mint"; $("n-type").dispatchEvent(new dom.window.Event("change"));
    assert.equal($("n-to-box").hidden, true);
    $("n-title").value = "Più moneta"; $("n-amount").value = "5000"; $("n-go").click();
    await until(() => articles().length === 2, "second proposal");
    assert.match(text(), /Se approvata \(servono almeno due terzi di sì tra sì e no\): emissione di 5000\.00 LUX/);
    $("n-type").value = "works"; $("n-type").dispatchEvent(new dom.window.Event("change"));
    assert.equal($("n-to-box").hidden, false);
    $("n-title").value = "Il parco"; $("n-amount").value = "12,50"; $("n-to").value = "mengano"; $("n-go").click();
    await until(() => articles().length === 3, "third proposal");
    assert.match(text(), /Se approvata: 12\.50 LUX a mengano/);

    // a bad recipient says why
    $("n-title").value = "Fantasma"; $("n-amount").value = "1"; $("n-to").value = "nessuno"; $("n-go").click();
    await until(() => $("n-msg").textContent !== "", "error");
    assert.match($("n-msg").textContent, /non è un membro/);
    assert.equal(articles().length, 3);

    // the others vote with their own keys (the ballot is open: everyone is listed)
    const list = (await members.fulano.call("GET", "/api/proposals")).json;
    const first = list.find((p) => p.title.includes("albero")).id;
    assert.equal((await apiVote(members.mengano, wb, first, "yes")).status, 200);
    assert.equal((await apiVote(members.zutano, wc, first, "abstain")).status, 200);
    dom = await openPage(node.base, members.fulano, idb); doms.push(dom);
    d = dom.window.document;
    await until(() => /Sì 1 · No 1 · Astenuti 1/.test(d.getElementById("p-list").textContent), "all votes");
    assert.match(d.getElementById("p-list").textContent, /Chi ha votato \(3\)/);
    assert.match(d.getElementById("p-list").textContent, /mengano: Sì/);
  } finally {
    doms.forEach((x) => x.window.close());
    await node.stop().catch(() => {});
  }
});
