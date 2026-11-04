// SPDX-License-Identifier: GPL-2.0-or-later
// openpolis wallet page (ADR-0020). The key is made in this browser from the 12-word phrase and never leaves it:
// it is stored as a non-extractable WebCrypto key in IndexedDB. Everything shown is written with textContent.
(function () {
  "use strict";
  var W = window.OpenpolisWallet;
  function $(id) { return document.getElementById(id); }
  function show(id, on) { $(id).hidden = !on; }
  function say(id, text) { $(id).textContent = text || ""; }

  async function api(path, method, body) {
    var r = await fetch(path, { method: method, credentials: "same-origin",
      headers: body ? { "Content-Type": "application/json" } : {}, body: body ? JSON.stringify(body) : undefined });
    var j = {}; try { j = await r.json(); } catch (e) {}
    if (!r.ok) { var err = new Error(j.error || ("Errore " + r.status)); err.status = r.status; throw err; }
    return j;
  }

  function db() {
    return new Promise(function (resolve, reject) {
      var r = indexedDB.open("openpolis", 1);
      r.onupgradeneeded = function () { r.result.createObjectStore("keys"); };
      r.onsuccess = function () { resolve(r.result); };
      r.onerror = function () { reject(r.error); };
    });
  }
  async function saveKey(id, value) {
    var d = await db();
    return new Promise(function (resolve, reject) {
      var tx = d.transaction("keys", "readwrite"); tx.objectStore("keys").put(value, id);
      tx.oncomplete = function () { resolve(); }; tx.onerror = function () { reject(tx.error); };
    });
  }
  async function loadKey(id) {
    var d = await db();
    return new Promise(function (resolve, reject) {
      var q = d.transaction("keys", "readonly").objectStore("keys").get(id);
      q.onsuccess = function () { resolve(q.result || null); }; q.onerror = function () { reject(q.error); };
    });
  }

  var wallet = null, pending = null, positions = [];
  function keyId() { return wallet.network_id + ":" + wallet.username; }
  function money(c) { return W.formatAmount(c) + " " + wallet.currency_symbol; }
  function when(t) { return new Date(t * 1000).toLocaleString(); }
  function plain(s) { return s.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().trim(); }

  function render() {
    var w = wallet, b = w.bank;
    $("w-balance").textContent = money(w.balance);
    $("w-debt").textContent = money(w.debt);
    $("w-fido").textContent = w.started ? money(w.fido) : "— (parte con la prima transazione)";
    $("w-credit").textContent = w.started ? money(w.credit_available) : "—";
    say("w-status", w.blocked ? "Il tuo conto è bloccato da un debito scaduto: puoi ricevere LUX, non spenderli." : "");
    $("b-cash").textContent = money(b.cash);
    $("b-required").textContent = money(b.required);
    $("b-surplus").textContent = (b.surplus < 0 ? "−" : "") + money(Math.abs(b.surplus));
    $("b-advance").textContent = money(b.advance_outstanding);
    $("b-loss").textContent = money(b.loss_target - b.loss_held) + " (trattenuti: " + money(b.loss_held) + ")";
    $("b-emitted").textContent = money(b.emitted_total);
    $("b-mint").textContent = money(b.mint_balance);
    $("b-members").textContent = String(b.members);
    $("b-cycles").textContent = String(b.cycles_closed);
    $("b-next").textContent = when(b.next_cycle_at);
    var tb = $("moves"); tb.textContent = "";
    w.recent.forEach(function (m) {
      var tr = document.createElement("tr");
      [when(m.time), m.from, m.to, money(m.amount), m.memo].forEach(function (v) {
        var td = document.createElement("td"); td.textContent = v; tr.appendChild(td);
      });
      tb.appendChild(tr);
    });
  }

  async function load() {
    try { wallet = await api("/api/wallet", "GET"); }
    catch (e) { if (e.status === 401) { location.href = "/login"; return; } say("w-status", e.message); return; }
    render();
    var stored = null;
    try { stored = await loadKey(keyId()); } catch (e) {}
    var mine = !!(stored && wallet.public_key && stored.publicKeyHex === wallet.public_key);
    show("w-create", !wallet.public_key);
    show("w-restore", !!wallet.public_key && !mine);
    show("w-pay", mine);
  }

  $("c-start").addEventListener("click", async function () {
    say("c-msg", "");
    var lang = (navigator.language || "en").slice(0, 2);
    if (W.LANGS.indexOf(lang) < 0) lang = "en";
    var entropy = W.generateEntropy();
    pending = { entropy: entropy, words: await W.entropyToWords(entropy, lang) };
    var ol = $("c-words"); ol.textContent = "";
    pending.words.forEach(function (w) { var li = document.createElement("li"); li.textContent = w; ol.appendChild(li); });
    positions = [];
    while (positions.length < 3) { var p = Math.floor(Math.random() * 12); if (positions.indexOf(p) < 0) positions.push(p); }
    positions.sort(function (a, b) { return a - b; });
    positions.forEach(function (p, i) { $("c-l" + (i + 1)).textContent = "Parola n. " + (p + 1); $("c-w" + (i + 1)).value = ""; });
    show("c-box", true); $("c-start").hidden = true;
  });

  $("c-create").addEventListener("click", async function () {
    say("c-msg", "");
    try {
      if (!pending) throw new Error("Genera prima la frase.");
      for (var i = 0; i < 3; i++) {
        if (plain($("c-w" + (i + 1)).value) !== plain(pending.words[positions[i]]))
          throw new Error("Le parole non coincidono: controlla di aver scritto bene la frase.");
      }
      var k = await W.deriveKeys(pending.entropy, wallet.network_id);
      var proof = await W.sign(k.privateKey, W.registrationMessage(wallet.network_id, wallet.username, k.publicKeyHex));
      await api("/api/me/key", "POST", { public_key: k.publicKeyHex, signature: proof, password: $("c-pass").value });
      await saveKey(keyId(), { publicKeyHex: k.publicKeyHex, privateKey: k.privateKey });
      pending = null; $("c-words").textContent = ""; $("c-pass").value = "";
      for (var j = 1; j <= 3; j++) $("c-w" + j).value = "";
      show("c-box", false); $("c-start").hidden = false;
      await load();
    } catch (err) { say("c-msg", err.message); }
  });

  $("r-go").addEventListener("click", async function () {
    say("r-msg", "");
    try {
      var r = await W.wordsToEntropy($("r-phrase").value);
      var k = await W.deriveKeys(r.entropy, wallet.network_id);
      if (k.publicKeyHex !== wallet.public_key) throw new Error("Questa frase non corrisponde al portafoglio di questo account.");
      await saveKey(keyId(), { publicKeyHex: k.publicKeyHex, privateKey: k.privateKey });
      $("r-phrase").value = "";
      await load();
    } catch (err) { say("r-msg", err.message); }
  });

  $("p-go").addEventListener("click", async function () {
    say("p-msg", ""); say("p-ok", "");
    try {
      var to = $("p-to").value.trim().toLowerCase(), amount = W.parseAmount($("p-amount").value);
      if (!to) throw new Error("Scrivi a chi vuoi pagare.");
      if (!amount) throw new Error("Importo non valido (esempio: 12,50).");
      var stored = await loadKey(keyId());
      if (!stored) throw new Error("Questo browser non ha la chiave: ripristina il portafoglio.");
      var payload = W.payPayload({ net: wallet.network_id, from: wallet.username, to: to, amountCenti: amount,
        nonce: wallet.next_nonce, memo: $("p-memo").value.slice(0, 80), t: Math.floor(Date.now() / 1000) });
      var r = await api("/api/pay", "POST", { payload: payload, signature: await W.sign(stored.privateKey, payload) });
      say("p-ok", "Inviato: hai pagato " + money(r.cost) + " (" + money(r.seller_receives) + " arrivano a " + to + ").");
      $("p-amount").value = ""; $("p-memo").value = "";
      await load();
    } catch (err) { say("p-msg", err.message); }
  });

  load();
})();
