// SPDX-License-Identifier: GPL-2.0-or-later
// openpolis parliament page (ADR-0021). Proposals and votes are signed in this browser with the wallet key
// (the same one as the wallet page). Everything shown is written with textContent.
(function () {
  "use strict";
  var W = window.OpenpolisWallet;
  function $(id) { return document.getElementById(id); }
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
  async function loadKey(id) {
    var d = await db();
    return new Promise(function (resolve, reject) {
      var q = d.transaction("keys", "readonly").objectStore("keys").get(id);
      q.onsuccess = function () { resolve(q.result || null); }; q.onerror = function () { reject(q.error); };
    });
  }

  var wallet = null, stored = null;
  var STATUS = { open: "In corso", accepted: "Approvata", rejected: "Respinta", no_quorum: "Senza quorum", void: "Annullata (il membro è tornato attivo)" };
  var KIND = { text: "Decisione", mint: "Emissione di moneta", works: "Lavoro sociale", expense: "Spesa di rete", abandon: "Abbandono di un membro" };
  var CHOICE = { yes: "Sì", no: "No", abstain: "Astenuto" };
  function when(t) { return new Date(t * 1000).toLocaleString(); }
  function money(c) { return W.formatAmount(c) + " " + wallet.currency_symbol; }
  function el(tag, text, cls) { var e = document.createElement(tag); if (text !== undefined) e.textContent = text; if (cls) e.className = cls; return e; }

  function card(p) {
    var a = el("article"); a.style.cssText = "border:1px solid #ccc;border-radius:.4rem;padding:.7rem;margin:.8rem 0";
    a.appendChild(el("h3", "#" + p.id + " · " + p.title));
    a.appendChild(el("p", KIND[p.kind] + " · proposta di " + p.proposer + " · " + (p.status === "open" ? "scade " : "chiusa il ") + when(p.deadline), "muted"));
    if (p.body) { var b = el("p", p.body); b.style.whiteSpace = "pre-wrap"; a.appendChild(b); }
    if (p.kind === "mint") a.appendChild(el("p", "Se approvata (servono almeno due terzi di sì tra sì e no): emissione di " + money(p.amount) + "."));
    if (p.kind === "abandon") a.appendChild(el("p", "Se approvata: l'account di " + p.to + " si chiude al prossimo ciclo, i suoi debiti scoperti sono cancellati e i LUX rimasti tornano alla cassa comune. Non vale se " + p.to + " firma qualcosa prima della chiusura."));
    if (p.kind === "works") a.appendChild(el("p", "Se approvata: " + money(p.amount) + " a " + p.to + ", pagati dal surplus della banca alla prossima chiusura di ciclo."));
    if (p.kind === "expense") a.appendChild(el("p", "Se approvata: " + money(p.amount) + " a " + p.to + " alla prossima chiusura di ciclo."));
    var voted = p.yes + p.no + p.abstain;
    a.appendChild(el("p", STATUS[p.status] + " — Sì " + p.yes + " · No " + p.no + " · Astenuti " + p.abstain +
      " — hanno votato " + voted + " su " + p.eligible + " (ne servono almeno " + p.quorum_needed + ")"));
    if (p.my_vote) a.appendChild(el("p", "Il tuo voto: " + CHOICE[p.my_vote], "muted"));
    if (p.can_vote && stored) {
      ["yes", "no", "abstain"].forEach(function (c) {
        var btn = el("button", CHOICE[c]); btn.type = "button"; btn.style.marginRight = ".5rem";
        btn.addEventListener("click", function () { vote(p.id, c); });
        a.appendChild(btn);
      });
    }
    if (p.votes.length) {
      var d = el("details"); d.appendChild(el("summary", "Chi ha votato (" + p.votes.length + ")"));
      p.votes.forEach(function (v) { d.appendChild(el("div", v.user + ": " + CHOICE[v.choice])); });
      a.appendChild(d);
    }
    return a;
  }

  async function load() {
    try { wallet = await api("/api/wallet", "GET"); }
    catch (e) { if (e.status === 401) { location.href = "/login"; return; } say("g-msg", e.message); return; }
    try { stored = await loadKey(wallet.network_id + ":" + wallet.username); } catch (e) { stored = null; }
    if (stored && stored.publicKeyHex !== wallet.public_key) stored = null;
    $("n-box").hidden = !stored;
    $("n-nokey").hidden = !!stored;
    var list = await api("/api/proposals", "GET"), box = $("p-list"); box.textContent = "";
    if (!list.length) box.appendChild(el("p", "Non c'è ancora nessuna proposta.", "muted"));
    list.forEach(function (p) { box.appendChild(card(p)); });
  }

  async function submit(path, payload) {
    return api(path, "POST", { payload: payload, signature: await W.sign(stored.privateKey, payload) });
  }

  async function vote(id, choice) {
    say("g-msg", "");
    try {
      await submit("/api/vote", W.votePayload({ net: wallet.network_id, from: wallet.username, proposal: id, choice: choice, nonce: wallet.next_nonce, t: Math.floor(Date.now() / 1000) }));
      await load();
    } catch (err) { say("g-msg", err.message); }
  }

  function syncForm() {
    var t = $("n-type").value;
    $("n-amount-box").hidden = t === "text" || t === "abandon";
    $("n-to-box").hidden = t === "text" || t === "mint";
  }
  $("n-type").addEventListener("change", syncForm);
  syncForm();

  $("n-go").addEventListener("click", async function () {
    say("n-msg", "");
    try {
      var type = $("n-type").value, amount = 0;
      if (type === "mint" || type === "works" || type === "expense") { amount = W.parseAmount($("n-amount").value); if (!amount) throw new Error("Importo non valido (esempio: 100 oppure 12,50)."); }
      var needsTo = type === "works" || type === "expense" || type === "abandon";
      var to = needsTo ? $("n-to").value.trim().toLowerCase() : "";
      if (needsTo && !to) throw new Error(type === "abandon" ? "Scrivi il nome del membro." : "Scrivi a chi va pagato.");
      var title = $("n-title").value.trim();
      if (!title) throw new Error("Scrivi un titolo.");
      await submit("/api/proposals", W.proposalPayload({ net: wallet.network_id, from: wallet.username, type: type, title: title, body: $("n-body").value,
        amountCenti: amount, to: to, days: parseInt($("n-days").value, 10), nonce: wallet.next_nonce, t: Math.floor(Date.now() / 1000) }));
      $("n-title").value = ""; $("n-body").value = ""; $("n-amount").value = ""; $("n-to").value = "";
      await load();
    } catch (err) { say("n-msg", err.message); }
  });

  load();
})();
