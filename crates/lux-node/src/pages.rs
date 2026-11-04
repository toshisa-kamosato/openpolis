// SPDX-License-Identifier: GPL-2.0-or-later
//! The two pages of this slice: administrator and member sign-up. They are plain HTML + one small script.
use crate::node::NetworkInfo;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

const STYLE: &str = "body{font:16px/1.5 system-ui,sans-serif;max-width:30rem;margin:2rem auto;padding:0 1rem;color:#111}\
h1{font-size:1.5rem}label{display:block;margin:.8rem 0 .2rem;font-weight:600}\
input,select{width:100%;padding:.55rem;font:inherit;box-sizing:border-box}\
button{margin-top:1rem;padding:.6rem 1.1rem;font:inherit;cursor:pointer}\
.msg{color:#b00020;min-height:1.4em}.pin{font:700 2.4rem monospace;letter-spacing:.2em;text-align:center;margin:1rem 0}\
table{width:100%;border-collapse:collapse;font-size:.85rem}td,th{padding:.25rem;text-align:left;border-bottom:1px solid #ddd}\
.muted{color:#666;font-size:.9rem}";

fn shell(title: &str, body: &str) -> String {
    shell_scripts(title, body, &["/static/app.js"])
}

fn shell_scripts(title: &str, body: &str, scripts: &[&str]) -> String {
    let tags: String = scripts.iter().map(|s| format!("<script src=\"{s}\"></script>")).collect();
    format!(
        "<!doctype html><html lang=\"it\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>{}</title><style>{STYLE}</style></head><body>{body}{tags}</body></html>",
        esc(title)
    )
}

pub fn index(n: &NetworkInfo) -> String {
    shell(
        &n.name,
        &format!(
            "<h1>{}</h1><p class=\"muted\">{} · {} membri · moneta {} ({})</p>\
<p><a href=\"/join\">Entra con un PIN</a> · <a href=\"/login\">Accedi</a> · <a href=\"/admin\">Amministrazione</a></p>",
            esc(&n.name), esc(&n.domain), n.members, esc(&n.currency_name), esc(&n.currency_symbol)
        ),
    )
}

pub fn join(n: &NetworkInfo) -> String {
    shell(
        &format!("Entra in {}", n.name),
        &format!(
            "<h1>Entra in {}</h1>\
<form id=\"join-form\"><label for=\"pin\">PIN di 6 cifre</label>\
<input id=\"pin\" inputmode=\"numeric\" pattern=\"[0-9]{{6}}\" maxlength=\"6\" autocomplete=\"one-time-code\" required>\
<label for=\"username\">Nome utente</label><input id=\"username\" autocomplete=\"username\" required>\
<label for=\"password\">Password (almeno 10 caratteri)</label>\
<input id=\"password\" type=\"password\" autocomplete=\"new-password\" required>\
<fieldset id=\"j-services\" style=\"margin-top:1rem\"><legend>Servizi che vuoi attivare</legend></fieldset>\
<p class=\"muted\">Portafoglio e voto nel parlamento ci sono sempre. Chi amministra il nodo vede in chiaro la posta (al momento della consegna), il blog e le chat che passano dai ponti.</p>\
<p class=\"msg\" id=\"msg\" role=\"alert\"></p><button type=\"submit\">Crea l'account</button></form>\
<div id=\"done\" hidden><p><strong>Benvenuto, <span id=\"done-name\"></span>!</strong> Il tuo account è stato creato.</p></div>",
            esc(&n.name)
        ),
    )
}

pub fn login(n: &NetworkInfo) -> String {
    shell(
        &format!("Accedi a {}", n.name),
        &format!(
            "<h1>Accedi a {}</h1>\
<form id=\"member-login-form\"><label for=\"m-user\">Nome utente</label><input id=\"m-user\" autocomplete=\"username\" required>\
<label for=\"m-pass\">Password</label><input id=\"m-pass\" type=\"password\" autocomplete=\"current-password\" required>\
<p class=\"msg\" id=\"m-msg\" role=\"alert\"></p><button type=\"submit\">Accedi</button></form>\
<p class=\"muted\">Non hai un account? <a href=\"/join\">Entra con un PIN</a>.</p>",
            esc(&n.name)
        ),
    )
}

pub fn me(n: &NetworkInfo) -> String {
    shell(
        &format!("Il mio spazio · {}", n.name),
        &format!(
            "<h1>Il mio spazio · {}</h1><p>Ciao, <strong id=\"me-name\">…</strong> <span class=\"muted\" id=\"me-role\"></span></p>\
<p><a href=\"/wallet\">Il mio portafoglio</a> · <a href=\"/parliament\">Il parlamento</a> · <a href=\"/services\">I miei servizi</a></p><p class=\"muted\">Moneta della rete: {} ({}). La posta e la chat arrivano nei prossimi passi.</p>\
<button id=\"logout\" type=\"button\">Esci</button>",
            esc(&n.name), esc(&n.currency_name), esc(&n.currency_symbol)
        ),
    )
}

pub fn wallet(n: &NetworkInfo) -> String {
    shell_scripts(
        &format!("Portafoglio · {}", n.name),
        &format!(
            "<h1>Portafoglio · {}</h1><p class=\"muted\"><a href=\"/me\">← Il mio spazio</a></p>\
<dl id=\"w-summary\"><dt>Saldo</dt><dd id=\"w-balance\">…</dd><dt>Debito</dt><dd id=\"w-debt\">…</dd>\
<dt>Fido</dt><dd id=\"w-fido\">…</dd><dt>Ancora disponibile</dt><dd id=\"w-credit\">…</dd></dl>\
<p class=\"msg\" id=\"w-status\" role=\"status\"></p>\
<section id=\"w-create\" hidden><h2>Crea il tuo portafoglio</h2>\
<p>Il portafoglio ha una chiave che nasce <strong>nel tuo browser</strong> da una frase di 12 parole. Il nodo non la vede mai: \
chi ha la frase ha i tuoi LUX, e se la perdi nessuno può restituirteli. Scrivila su carta.</p>\
<button id=\"c-start\" type=\"button\">Genera la mia frase di recupero</button>\
<div id=\"c-box\" hidden><ol id=\"c-words\"></ol>\
<p>Per essere sicuri che l'hai scritta, riscrivi queste parole:</p>\
<label for=\"c-w1\" id=\"c-l1\"></label><input id=\"c-w1\" autocomplete=\"off\" autocapitalize=\"none\">\
<label for=\"c-w2\" id=\"c-l2\"></label><input id=\"c-w2\" autocomplete=\"off\" autocapitalize=\"none\">\
<label for=\"c-w3\" id=\"c-l3\"></label><input id=\"c-w3\" autocomplete=\"off\" autocapitalize=\"none\">\
<label for=\"c-pass\">La tua password (conferma)</label><input id=\"c-pass\" type=\"password\" autocomplete=\"current-password\">\
<button id=\"c-create\" type=\"button\">Crea il portafoglio</button></div>\
<p class=\"msg\" id=\"c-msg\" role=\"alert\"></p></section>\
<section id=\"w-restore\" hidden><h2>Usa il portafoglio su questo browser</h2>\
<p>Questo browser non ha la chiave. Scrivi le 12 parole della tua frase di recupero.</p>\
<textarea id=\"r-phrase\" rows=\"3\" autocomplete=\"off\" autocapitalize=\"none\" style=\"width:100%;font:inherit\"></textarea>\
<button id=\"r-go\" type=\"button\">Ripristina</button><p class=\"msg\" id=\"r-msg\" role=\"alert\"></p></section>\
<section id=\"w-pay\" hidden><h2>Paga</h2>\
<label for=\"p-to\">A (nome utente)</label><input id=\"p-to\" autocomplete=\"off\">\
<label for=\"p-amount\">Importo</label><input id=\"p-amount\" inputmode=\"decimal\" autocomplete=\"off\">\
<label for=\"p-memo\">Nota (facoltativa)</label><input id=\"p-memo\" maxlength=\"80\" autocomplete=\"off\">\
<p class=\"muted\">Ogni movimento tra utenti paga il 5% (2,5% chi paga, 2,5% chi riceve).</p>\
<button id=\"p-go\" type=\"button\">Firma e invia</button><p class=\"msg\" id=\"p-msg\" role=\"alert\"></p><p id=\"p-ok\" role=\"status\"></p></section>\
<h2>Movimenti</h2><table><thead><tr><th>Quando</th><th>Da</th><th>A</th><th>Importo</th><th>Nota</th></tr></thead><tbody id=\"moves\"></tbody></table>\
<h2>La banca</h2><dl id=\"bank\">\
<dt>Cassa comune</dt><dd id=\"b-cash\"></dd><dt>Copertura dei fidi</dt><dd id=\"b-required\"></dd><dt>Surplus</dt><dd id=\"b-surplus\"></dd>\
<dt>Anticipo dalla zecca da restituire</dt><dd id=\"b-advance\"></dd><dt>Perdite da assorbire</dt><dd id=\"b-loss\"></dd>\
<dt>Moneta emessa</dt><dd id=\"b-emitted\"></dd><dt>Ancora nella zecca</dt><dd id=\"b-mint\"></dd>\
<dt>Iscritti</dt><dd id=\"b-members\"></dd><dt>Cicli chiusi</dt><dd id=\"b-cycles\"></dd><dt>Prossima chiusura</dt><dd id=\"b-next\"></dd></dl>",
            esc(&n.name)
        ),
        &["/static/wordlists.js", "/static/wallet-core.js", "/static/wallet.js"],
    )
}

pub fn parliament(n: &NetworkInfo) -> String {
    shell_scripts(
        &format!("Parlamento · {}", n.name),
        &format!(
            "<h1>Parlamento · {}</h1><p class=\"muted\"><a href=\"/me\">← Il mio spazio</a> · <a href=\"/wallet\">Portafoglio</a></p>\
<p class=\"msg\" id=\"g-msg\" role=\"alert\"></p>\
<section id=\"n-box\" hidden><h2>Nuova proposta</h2>\
<label for=\"n-type\">Che cosa proponi?</label><select id=\"n-type\">\
<option value=\"text\">Una decisione o una consultazione (nessun effetto automatico)</option>\
<option value=\"mint\">Emettere nuova moneta</option>\
<option value=\"works\">Un lavoro sociale (pagato dal surplus della banca)</option>\
<option value=\"expense\">Una spesa di gestione della rete</option>\
<option value=\"abandon\">Dichiarare abbandonato un membro che non firma nulla da 84 giorni</option></select>\
<label for=\"n-title\">Titolo</label><input id=\"n-title\" maxlength=\"120\" autocomplete=\"off\">\
<label for=\"n-body\">Testo</label><textarea id=\"n-body\" rows=\"4\" maxlength=\"2000\" style=\"width:100%;font:inherit\"></textarea>\
<div id=\"n-amount-box\" hidden><label for=\"n-amount\">Importo</label><input id=\"n-amount\" inputmode=\"decimal\" autocomplete=\"off\"></div>\
<div id=\"n-to-box\" hidden><label for=\"n-to\">A chi (nome utente)</label><input id=\"n-to\" autocomplete=\"off\"></div>\
<label for=\"n-days\">Durata della votazione</label><select id=\"n-days\"><option value=\"1\">1 giorno</option><option value=\"3\">3 giorni</option>\
<option value=\"7\" selected>7 giorni</option><option value=\"14\">14 giorni</option><option value=\"28\">28 giorni</option></select>\
<p class=\"muted\">Vota chi ha già il portafoglio quando la proposta si apre. Serve che voti almeno il 20% degli aventi diritto e che i «sì» siano più dei «no» (per emettere moneta, almeno due terzi). Il voto è aperto: tutti vedono chi ha votato cosa.</p>\
<button id=\"n-go\" type=\"button\">Firma e proponi</button><p class=\"msg\" id=\"n-msg\" role=\"alert\"></p></section>\
<p id=\"n-nokey\" class=\"muted\" hidden>Per proporre e votare serve il portafoglio su questo browser: <a href=\"/wallet\">crealo o ripristinalo qui</a>.</p>\
<h2>Proposte</h2><div id=\"p-list\"></div>",
            esc(&n.name)
        ),
        &["/static/wordlists.js", "/static/wallet-core.js", "/static/parliament.js"],
    )
}

pub fn services(n: &NetworkInfo) -> String {
    shell(
        &format!("I miei servizi · {}", n.name),
        &format!(
            "<h1>I miei servizi · {}</h1><p class=\"muted\"><a href=\"/me\">← Il mio spazio</a></p>\
<ul id=\"svc-list\"></ul><p class=\"msg\" id=\"svc-msg\" role=\"alert\"></p>\
<form id=\"svc-form\" hidden><h2>Attiva altri servizi o riprova</h2><fieldset id=\"svc-boxes\" style=\"border:0;padding:0\"></fieldset>\
<label for=\"svc-pass\">La tua password (serve per creare gli account; non viene conservata)</label>\
<input id=\"svc-pass\" type=\"password\" autocomplete=\"current-password\" required><button type=\"submit\">Attiva</button></form>",
            esc(&n.name)
        ),
    )
}

pub fn admin(n: &NetworkInfo) -> String {
    shell(
        &format!("Amministrazione · {}", n.name),
        &format!(
            "<h1>Amministrazione · {}</h1>\
<form id=\"login-form\"><label for=\"a-user\">Utente</label><input id=\"a-user\" autocomplete=\"username\" required>\
<label for=\"a-pass\">Password</label><input id=\"a-pass\" type=\"password\" autocomplete=\"current-password\" required>\
<p class=\"msg\" id=\"login-msg\" role=\"alert\"></p><button type=\"submit\">Accedi</button></form>\
<div id=\"panel\" hidden><h2>Nuovo invito</h2>\
<label for=\"ttl\">Validità</label><select id=\"ttl\"><option value=\"24\">24 ore</option><option value=\"72\">3 giorni</option>\
<option value=\"168\">7 giorni</option><option value=\"1\">1 ora</option></select>\
<button id=\"new-pin\" type=\"button\">Genera PIN</button><p class=\"msg\" id=\"panel-msg\" role=\"alert\"></p>\
<div class=\"pin\" id=\"pin-out\"></div><p class=\"muted\" id=\"pin-note\"></p>\
<h2>Inviti recenti</h2><table><thead><tr><th>#</th><th>Scade</th><th>Stato</th></tr></thead><tbody id=\"pins\"></tbody></table></div>",
            esc(&n.name)
        ),
    )
}

pub const APP_JS: &str = r#"(function () {
  function $(id) { return document.getElementById(id); }
  async function api(path, method, body) {
    var r = await fetch(path, { method: method, credentials: "same-origin",
      headers: body ? { "Content-Type": "application/json" } : {}, body: body ? JSON.stringify(body) : undefined });
    var j = {}; try { j = await r.json(); } catch (e) {}
    if (!r.ok) throw new Error(j.error || ("Errore " + r.status));
    return j;
  }
  var LABELS = { subdomain: "Il tuo sottodominio (nome.dominio)", mail: "Posta elettronica (1 GB)", chat: "Chat Matrix (con ponti a Telegram e WhatsApp)", insta: "Blog e galleria (3 GB)" };
  function boxes(container, names, prefix) {
    container.textContent = "";
    names.forEach(function (s) {
      var lab = document.createElement("label"); lab.style.fontWeight = "400";
      var cb = document.createElement("input"); cb.type = "checkbox"; cb.value = s; cb.id = prefix + s; cb.checked = true; cb.style.width = "auto"; cb.style.marginRight = ".5rem";
      lab.appendChild(cb); lab.appendChild(document.createTextNode(LABELS[s] || s)); container.appendChild(lab);
    });
  }
  function chosen(container) { return Array.prototype.filter.call(container.querySelectorAll("input"), function (c) { return c.checked; }).map(function (c) { return c.value; }); }
  var jf = $("join-form");
  if (jf) api("/api/network", "GET").then(function (n) { boxes($("j-services"), n.services || [], "svc-"); }, function () {});
  if (jf) jf.addEventListener("submit", async function (e) {
    e.preventDefault(); $("msg").textContent = "";
    try {
      var j = await api("/api/join", "POST", { pin: $("pin").value, username: $("username").value, password: $("password").value, services: chosen($("j-services")) });
      jf.hidden = true; $("done").hidden = false; $("done-name").textContent = j.username;
    } catch (err) { $("msg").textContent = err.message; }
  });
  var mf = $("member-login-form");
  if (mf) mf.addEventListener("submit", async function (e) {
    e.preventDefault(); $("m-msg").textContent = "";
    try { await api("/api/login", "POST", { username: $("m-user").value, password: $("m-pass").value }); location.href = "/me"; }
    catch (err) { $("m-msg").textContent = err.message; }
  });
  if ($("me-name")) {
    api("/api/me", "GET").then(function (u) { $("me-name").textContent = u.username; $("me-role").textContent = u.role === "admin" ? "(amministratore)" : ""; },
      function () { location.href = "/login"; });
    $("logout").addEventListener("click", async function () { try { await api("/api/logout", "POST", {}); } catch (e) {} location.href = "/login"; });
  }
  if ($("svc-list")) {
    var STATE = { active: "attivo", requested: "in attesa", failed: "non riuscito" };
    var loadServices = async function () {
      var s;
      try { s = await api("/api/services", "GET"); } catch (err) { location.href = "/login"; return; }
      var ul = $("svc-list"); ul.textContent = "";
      s.mine.forEach(function (m) {
        var li = document.createElement("li"); li.appendChild(document.createTextNode(m.label + ": " + (STATE[m.status] || m.status) + (m.detail ? " — " + m.detail : "") + " "));
        if (m.link && m.link.indexOf("https://") === 0) { var a = document.createElement("a"); a.href = m.link; a.textContent = m.link; li.appendChild(a); }
        ul.appendChild(li);
      });
      if (!s.mine.length) { var li0 = document.createElement("li"); li0.textContent = "Non hai attivato nessun servizio."; ul.appendChild(li0); }
      var todo = s.offered.filter(function (o) { return !s.mine.some(function (m) { return m.service === o && m.status === "active"; }); });
      $("svc-form").hidden = !todo.length; boxes($("svc-boxes"), todo, "more-");
    };
    $("svc-form").addEventListener("submit", async function (e) {
      e.preventDefault(); $("svc-msg").textContent = "";
      try { await api("/api/services", "POST", { password: $("svc-pass").value, services: chosen($("svc-boxes")) }); $("svc-pass").value = ""; await loadServices(); }
      catch (err) { $("svc-msg").textContent = err.message; }
    });
    loadServices();
  }
  var lf = $("login-form");
  function when(t) { return new Date(t * 1000).toLocaleString(); }
  async function loadPins() {
    var list = await api("/api/admin/pins", "GET"), tb = $("pins"); tb.textContent = "";
    list.forEach(function (p) {
      var tr = document.createElement("tr"), now = Date.now() / 1000;
      var st = p.used_at ? "usato da " + (p.used_by || "?") : (p.expires_at < now ? "scaduto" : "attivo");
      [p.id, when(p.expires_at), st].forEach(function (v) { var td = document.createElement("td"); td.textContent = v; tr.appendChild(td); });
      tb.appendChild(tr);
    });
  }
  if (lf) {
    lf.addEventListener("submit", async function (e) {
      e.preventDefault(); $("login-msg").textContent = "";
      try {
        await api("/api/admin/login", "POST", { username: $("a-user").value, password: $("a-pass").value });
        lf.hidden = true; $("panel").hidden = false; await loadPins();
      } catch (err) { $("login-msg").textContent = err.message; }
    });
    $("new-pin").addEventListener("click", async function () {
      $("panel-msg").textContent = "";
      try {
        var j = await api("/api/admin/pins", "POST", { ttl_hours: parseInt($("ttl").value, 10) });
        $("pin-out").textContent = j.pin;
        $("pin-note").textContent = "Consegnalo a mano o su un canale cifrato. Non verrà mostrato di nuovo. Scade: " + when(j.expires_at);
        await loadPins();
      } catch (err) { $("panel-msg").textContent = err.message; }
    });
    api("/api/admin/pins", "GET").then(function () { lf.hidden = true; $("panel").hidden = false; return loadPins(); }, function () {});
  }
})();
"#;
