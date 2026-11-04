// SPDX-License-Identifier: GPL-2.0-or-later
// The wallet page (static/wallet.js) driven like a person would: jsdom + fake IndexedDB + Node's WebCrypto,
// against a real `luxd` process. It does NOT replace a try-out in real browsers (Chrome, Firefox, Safari).
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

async function openWalletPage(base, client, idb, language) {
  const html = (await client.call("GET", "/wallet")).text;
  const dom = new JSDOM(html, { url: base + "/wallet", runScripts: "outside-only", pretendToBeVisual: true });
  const w = dom.window;
  Object.defineProperty(w, "crypto", { value: nodeCrypto.webcrypto, configurable: true });
  Object.defineProperty(w.navigator, "language", { value: language, configurable: true });
  w.TextEncoder = TextEncoder;
  w.indexedDB = idb;
  w.fetch = (url, opts = {}) => fetch(new URL(url, base), { ...opts, headers: { ...(opts.headers || {}), Cookie: client.header() } });
  for (const f of ["wordlists.js", "wallet-core.js", "wallet.js"]) w.eval(read(f));
  return dom;
}

async function until(fn, what, ms = 8000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (fn()) return; await new Promise((r) => setTimeout(r, 25)); }
  throw new Error("timeout waiting for: " + what);
}

test("create the wallet, pay, and restore it on another browser", async () => {
  const node = await startNode();
  const doms = [];
  try {
    const { members } = await makeMembers(node.base, ["fulano", "mengano"]);
    const a = members.fulano;

    // ---- a new browser: no wallet yet
    let dom = await openWalletPage(node.base, a, new IDBFactory(), "it-IT"); doms.push(dom);
    let d = dom.window.document;
    const $ = (id) => d.getElementById(id);
    await until(() => !$("w-create").hidden, "create panel");
    assert.ok($("w-pay").hidden && $("w-restore").hidden);
    assert.equal($("w-balance").textContent, "0.00 LUX");
    assert.equal($("b-members").textContent, "3");
    assert.equal($("b-cash").textContent, "8000.00 LUX", "5 000 + 3 x 1 000");

    // ---- the phrase: 12 Italian words
    $("c-start").click();
    await until(() => d.querySelectorAll("#c-words li").length === 12, "12 words");
    const words = [...d.querySelectorAll("#c-words li")].map((li) => li.textContent);
    assert.ok(words.every((w) => globalThis.OPENPOLIS_WORDLISTS.it.includes(w)), "an italian browser gets the italian list");
    const pos = [1, 2, 3].map((i) => parseInt($("c-l" + i).textContent.match(/\d+/)[0], 10) - 1);
    const fill = (right, pass) => {
      [1, 2, 3].forEach((i) => { $("c-w" + i).value = right ? words[pos[i - 1]] : "sbagliata"; });
      $("c-pass").value = pass;
    };
    const attempt = async (right, pass) => { $("c-msg").textContent = ""; fill(right, pass); $("c-create").click(); await until(() => $("c-msg").textContent !== "" || !$("w-pay").hidden, "result"); };

    await attempt(false, PW);
    assert.match($("c-msg").textContent, /non coincidono/);
    await attempt(true, "password sbagliata");
    assert.notEqual($("c-msg").textContent, "");
    assert.equal((await a.call("GET", "/api/wallet")).json.public_key, null, "nothing was registered yet");
    await attempt(true, PW);
    await until(() => !$("w-pay").hidden, "pay panel");
    assert.ok($("w-create").hidden);
    assert.equal($("c-words").children.length, 0, "the words are wiped from the page");
    const registered = (await a.call("GET", "/api/wallet")).json.public_key;
    assert.equal(registered.length, 64);

    // ---- a payment, signed in the page; the memo is shown as text, never as HTML
    $("p-to").value = "mengano"; $("p-amount").value = "10,00"; $("p-memo").value = "<img src=x onerror=alert(1)>";
    $("p-go").click();
    await until(() => $("p-ok").textContent.includes("Inviato"), "payment sent");
    await until(() => $("w-debt").textContent === "10.25 LUX", "debt shown");
    assert.equal(d.querySelectorAll("#moves tr").length, 1);
    assert.ok(!d.querySelector("#moves img"), "no HTML from the memo");
    assert.match(d.querySelector("#moves tr").textContent, /mengano.*10\.00 LUX/);
    assert.equal($("w-credit").textContent, "89.75 LUX");

    // an invalid payment says why and writes nothing
    $("p-to").value = "nessuno"; $("p-amount").value = "1"; $("p-go").click();
    await until(() => $("p-msg").textContent !== "", "error");
    assert.match($("p-msg").textContent, /inesistente/);

    // ---- another browser (empty IndexedDB): the phrase restores it, even written in Spanish, upper case, extra spaces
    dom = await openWalletPage(node.base, a, new IDBFactory(), "es-ES"); doms.push(dom);
    d = dom.window.document;
    await until(() => !d.getElementById("w-restore").hidden, "restore panel");
    assert.ok(d.getElementById("w-create").hidden && d.getElementById("w-pay").hidden);
    const r = (id) => d.getElementById(id);
    r("r-phrase").value = "abaco ".repeat(12); r("r-go").click();
    await until(() => r("r-msg").textContent !== "", "bad phrase");
    r("r-msg").textContent = "";
    const entropy = (await W.wordsToEntropy(words.join(" "))).entropy;
    const asSpanish = (await W.entropyToWords(entropy, "es")).join("   ").toUpperCase();
    r("r-phrase").value = asSpanish; r("r-go").click();
    await until(() => !r("w-pay").hidden, "restored");
    // and the restored key signs payments the node accepts (the nonce continues)
    r("p-to").value = "mengano"; r("p-amount").value = "1"; r("p-go").click();
    await until(() => r("p-ok").textContent.includes("Inviato"), "payment from the restored wallet");
    const w = (await a.call("GET", "/api/wallet")).json;
    assert.equal(w.recent.length, 2);
    assert.equal(w.next_nonce, 3);
  } finally {
    doms.forEach((x) => x.window.close());
    await node.stop().catch(() => {});
  }
});
