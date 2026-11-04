// SPDX-License-Identifier: GPL-2.0-or-later
// openpolis wallet, cryptographic core (browser and Node 20+). No network, no DOM.
//  * 12-word recovery phrase = 128 bits of entropy written with a BIP-39 list (it / en / es) + 4 checksum bits;
//  * the signing key (Ed25519) is derived from the 128 bits, not from the words: HKDF-SHA256 with the network id as salt,
//    so the same phrase gives a different key on every network and can be shown in any language;
//  * the private key is imported into WebCrypto as non-extractable: no script can read it back.
(function (root) {
  "use strict";
  var subtle = root.crypto && root.crypto.subtle;
  var enc = new TextEncoder();
  var PKCS8_ED25519_PREFIX = new Uint8Array([0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20]);
  var LANGS = ["it", "en", "es"];

  function hex(b) { return Array.prototype.map.call(b, function (x) { return ("0" + x.toString(16)).slice(-2); }).join(""); }
  function unhex(s) {
    if (!/^([0-9a-f]{2})*$/i.test(s)) throw new Error("hex non valido");
    var out = new Uint8Array(s.length / 2);
    for (var i = 0; i < out.length; i++) out[i] = parseInt(s.substr(i * 2, 2), 16);
    return out;
  }
  function b64urlToBytes(s) {
    s = s.replace(/-/g, "+").replace(/_/g, "/"); while (s.length % 4) s += "=";
    var bin = atob(s), out = new Uint8Array(bin.length);
    for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }
  function concat(a, b) { var o = new Uint8Array(a.length + b.length); o.set(a, 0); o.set(b, a.length); return o; }
  function strip(w) { return w.normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().trim(); }
  function lists() { return root.OPENPOLIS_WORDLISTS; }
  var cache = {};
  function index(lang) {
    if (!cache[lang]) { var m = new Map(); lists()[lang].forEach(function (w, i) { m.set(strip(w), i); }); cache[lang] = m; }
    return cache[lang];
  }
  async function sha256(bytes) { return new Uint8Array(await subtle.digest("SHA-256", bytes)); }
  function bitsOf(bytes) { return Array.prototype.map.call(bytes, function (x) { return ("0000000" + x.toString(2)).slice(-8); }).join(""); }

  function generateEntropy() { var b = new Uint8Array(16); root.crypto.getRandomValues(b); return b; }

  async function entropyToWords(entropy, lang) {
    if (entropy.length !== 16) throw new Error("servono 16 byte di entropia");
    if (!lists()[lang]) throw new Error("lingua sconosciuta");
    var bits = bitsOf(entropy) + bitsOf((await sha256(entropy)).subarray(0, 1)).slice(0, 4);
    var words = [];
    for (var i = 0; i < 12; i++) words.push(lists()[lang][parseInt(bits.slice(i * 11, (i + 1) * 11), 2)]);
    return words;
  }

  // Accepts 12 words typed by a person (any case, with or without accents). Returns { entropy, lang }.
  async function wordsToEntropy(input) {
    var words = (Array.isArray(input) ? input : String(input).split(/\s+/)).map(strip).filter(Boolean);
    if (words.length !== 12) throw new Error("La frase deve avere esattamente 12 parole.");
    var lastError = "Le parole non appartengono a una lista conosciuta.";
    for (var k = 0; k < LANGS.length; k++) {
      var idx = index(LANGS[k]), nums = words.map(function (w) { return idx.get(w); });
      if (nums.some(function (n) { return n === undefined; })) continue;
      var bits = nums.map(function (n) { return ("00000000000" + n.toString(2)).slice(-11); }).join("");
      var entropy = new Uint8Array(16);
      for (var i = 0; i < 16; i++) entropy[i] = parseInt(bits.slice(i * 8, i * 8 + 8), 2);
      var check = bits.slice(128, 132), want = bitsOf((await sha256(entropy)).subarray(0, 1)).slice(0, 4);
      if (check === want) return { entropy: entropy, lang: LANGS[k] };
      lastError = "La frase ha un errore: controlla le parole e il loro ordine.";
    }
    throw new Error(lastError);
  }

  async function deriveKeys(entropy, networkId) {
    var ikm = await subtle.importKey("raw", entropy, "HKDF", false, ["deriveBits"]);
    var seed = new Uint8Array(await subtle.deriveBits(
      { name: "HKDF", hash: "SHA-256", salt: enc.encode("openpolis/v1/" + networkId), info: enc.encode("ed25519-signing-key") }, ikm, 256));
    var pkcs8 = concat(PKCS8_ED25519_PREFIX, seed);
    var tmp = await subtle.importKey("pkcs8", pkcs8, { name: "Ed25519" }, true, ["sign"]);   // extractable only to read the public half
    var pub = b64urlToBytes((await subtle.exportKey("jwk", tmp)).x);
    var priv = await subtle.importKey("pkcs8", pkcs8, { name: "Ed25519" }, false, ["sign"]); // the key that is kept: non-extractable
    seed.fill(0);
    return { privateKey: priv, publicKeyHex: hex(pub) };
  }

  async function sign(privateKey, message) {
    return hex(new Uint8Array(await subtle.sign({ name: "Ed25519" }, privateKey, enc.encode(message))));
  }

  function registrationMessage(networkId, username, publicKeyHex) {
    return "register-key:v1:" + networkId + ":" + username + ":" + publicKeyHex;
  }

  // The exact string that is signed and stored in the ledger. Key order is fixed.
  function payPayload(p) {
    var o = { v: 1, net: p.net, kind: "pay", from: p.from, to: p.to, amount: p.amountCenti, nonce: p.nonce, memo: p.memo || "", t: p.t };
    return JSON.stringify(o);
  }

  // Proposals and votes: like payments, a fixed key order, signed as the exact string that goes into the ledger.
  function proposalPayload(p) {
    return JSON.stringify({ v: 1, net: p.net, kind: "proposal", from: p.from, type: p.type, title: p.title, body: p.body,
      amount: p.amountCenti || 0, to: p.to || "", days: p.days, nonce: p.nonce, t: p.t });
  }
  function votePayload(p) {
    return JSON.stringify({ v: 1, net: p.net, kind: "vote", from: p.from, proposal: p.proposal, choice: p.choice, nonce: p.nonce, t: p.t });
  }

  // "12", "12.3", "12,34" -> centi-LUX (integer) or null
  function parseAmount(text) {
    var m = /^\s*(\d{1,12})(?:[.,](\d{1,2}))?\s*$/.exec(String(text));
    if (!m) return null;
    var cents = parseInt(m[1], 10) * 100 + (m[2] ? parseInt((m[2] + "0").slice(0, 2), 10) : 0);
    return cents > 0 ? cents : null;
  }
  function formatAmount(centi) { return Math.floor(centi / 100) + "." + ("0" + (centi % 100)).slice(-2); }

  root.OpenpolisWallet = {
    LANGS: LANGS, hex: hex, unhex: unhex, generateEntropy: generateEntropy, entropyToWords: entropyToWords,
    wordsToEntropy: wordsToEntropy, deriveKeys: deriveKeys, sign: sign, registrationMessage: registrationMessage,
    payPayload: payPayload, proposalPayload: proposalPayload, votePayload: votePayload, parseAmount: parseAmount, formatAmount: formatAmount
  };
  if (typeof module !== "undefined" && module.exports) module.exports = root.OpenpolisWallet;
})(typeof window !== "undefined" ? window : globalThis);
