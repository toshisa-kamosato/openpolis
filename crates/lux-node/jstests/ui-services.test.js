// SPDX-License-Identifier: GPL-2.0-or-later
// The sign-up page (choice of services) and the "my services" page (static code in src/pages.rs), against a real
// `luxd` process. jsdom: it does NOT replace a try-out in real browsers.
const test = require("node:test");
const assert = require("node:assert/strict");
const { JSDOM } = require("jsdom");
const { startNode, Client, PW } = require("./support.js");

async function openPage(base, client, pagePath) {
  const html = (await client.call("GET", pagePath)).text;
  const js = (await client.call("GET", "/static/app.js")).text;
  const dom = new JSDOM(html, { url: base + pagePath, runScripts: "outside-only", pretendToBeVisual: true });
  const w = dom.window;
  w.fetch = (url, opts = {}) => fetch(new URL(url, base), { ...opts, headers: { ...(opts.headers || {}), Cookie: client.header() } });
  w.eval(js);
  return dom;
}
async function until(fn, what, ms = 8000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) { if (fn()) return; await new Promise((r) => setTimeout(r, 25)); }
  throw new Error("timeout waiting for: " + what);
}
const submit = (dom, id) => dom.window.document.getElementById(id).dispatchEvent(new dom.window.Event("submit", { bubbles: true, cancelable: true }));

test("choose services at sign-up, then add one from the services page", async () => {
  const node = await startNode();
  const doms = [];
  try {
    const admin = new Client(node.base);
    await admin.call("POST", "/api/admin/login", { username: "clara", password: PW });
    const pin = (await admin.call("POST", "/api/admin/pins", { ttl_hours: 24 })).json.pin;

    // ---- sign-up page: the services the node offers are checkboxes, all ticked
    let dom = await openPage(node.base, new Client(node.base), "/join"); doms.push(dom);
    let d = dom.window.document;
    await until(() => d.querySelectorAll("#j-services input").length === 4, "four services");
    assert.ok([...d.querySelectorAll("#j-services input")].every((c) => c.checked));
    assert.match(d.getElementById("j-services").textContent, /Posta elettronica \(1 GB\)/);
    assert.match(d.getElementById("j-services").textContent, /Blog e galleria \(3 GB\)/);
    d.getElementById("svc-chat").checked = false;
    d.getElementById("pin").value = pin; d.getElementById("username").value = "fulano"; d.getElementById("password").value = PW;
    submit(dom, "join-form");
    await until(() => !d.getElementById("done").hidden, "account created");

    // the node has what was chosen, and no chat
    const me = new Client(node.base);
    assert.equal((await me.call("POST", "/api/login", { username: "fulano", password: PW })).status, 200);
    let s = (await me.call("GET", "/api/services")).json;
    assert.deepEqual(s.mine.map((m) => m.service), ["subdomain", "mail", "insta"]);

    // ---- services page
    dom = await openPage(node.base, me, "/services"); doms.push(dom);
    d = dom.window.document;
    await until(() => d.querySelectorAll("#svc-list li").length === 3, "three services listed");
    assert.match(d.getElementById("svc-list").textContent, /Posta elettronica: in attesa/, "no manager is connected to this test node");
    assert.equal(d.getElementById("svc-form").hidden, false);
    await until(() => d.querySelectorAll("#svc-boxes input").length >= 1, "boxes");
    const left = [...d.querySelectorAll("#svc-boxes input")].map((c) => c.value);
    assert.ok(left.includes("chat"));

    // a wrong password changes nothing and says so
    [...d.querySelectorAll("#svc-boxes input")].forEach((c) => { c.checked = c.value === "chat"; });
    d.getElementById("svc-pass").value = "password sbagliata"; submit(dom, "svc-form");
    await until(() => d.getElementById("svc-msg").textContent !== "", "error");
    assert.equal(d.querySelectorAll("#svc-list li").length, 3);

    // the right one adds the chat
    d.getElementById("svc-pass").value = PW; submit(dom, "svc-form");
    await until(() => d.querySelectorAll("#svc-list li").length === 4, "chat added");
    assert.match(d.getElementById("svc-list").textContent, /Chat \(Matrix\)/);
    s = (await me.call("GET", "/api/services")).json;
    assert.equal(s.mine.length, 4);
  } finally {
    doms.forEach((x) => x.window.close());
    await node.stop().catch(() => {});
  }
});
