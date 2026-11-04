// SPDX-License-Identifier: GPL-2.0-or-later
// Starts a real `luxd` node in a temporary directory and gives a small cookie-aware HTTP client.
const { spawn, spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const LUXD = process.env.LUXD_BIN || path.join(process.env.CARGO_TARGET_DIR || path.join(__dirname, "../../../target"), "debug/luxd");
const PW = "una frase lunga e strana";

class Client {
  constructor(base) { this.base = base; this.cookies = new Map(); }
  header() { return [...this.cookies].map(([k, v]) => `${k}=${v}`).join("; "); }
  async call(method, p, body) {
    const res = await fetch(this.base + p, {
      method,
      headers: { ...(body !== undefined ? { "Content-Type": "application/json" } : {}), ...(this.cookies.size ? { Cookie: this.header() } : {}) },
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    for (const sc of res.headers.getSetCookie()) {
      const [kv] = sc.split(";"); const i = kv.indexOf("=");
      const k = kv.slice(0, i), v = kv.slice(i + 1);
      if (v === "") this.cookies.delete(k); else this.cookies.set(k, v);
    }
    const text = await res.text();
    let json = null; try { json = JSON.parse(text); } catch (e) {}
    return { status: res.status, json, text };
  }
}

async function startNode() {
  if (!fs.existsSync(LUXD)) throw new Error(`luxd not found at ${LUXD}: run \`cargo build -p luxd\` or set LUXD_BIN`);
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "luxd-e2e-"));
  const env = { ...process.env, LUX_ADMIN_PASSWORD: PW };
  const init = spawnSync(LUXD, ["--home", home, "init", "--name", "Comunidad de prueba", "--domain", "openpolis.site", "--admin-user", "clara"], { env, encoding: "utf8" });
  if (init.status !== 0) throw new Error("luxd init failed: " + init.stderr);
  const port = 20000 + Math.floor(Math.random() * 20000);
  const child = spawn(LUXD, ["--home", home, "run", "--listen", `127.0.0.1:${port}`], { env, stdio: "ignore" });
  const exited = new Promise((r) => child.once("exit", r));
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) {
    try { if ((await fetch(base + "/api/network")).ok) break; } catch (e) {}
    await new Promise((r) => setTimeout(r, 50));
  }
  return { base, home, stop: () => { child.kill(); return exited; }, luxd: (args) => spawnSync(LUXD, ["--home", home, ...args], { env, encoding: "utf8" }), restart: null };
}

// admin session + two or more members who have joined (no wallet yet)
async function makeMembers(base, names) {
  const admin = new Client(base);
  let r = await admin.call("POST", "/api/admin/login", { username: "clara", password: PW });
  if (r.status !== 200) throw new Error("admin login failed");
  const out = {};
  for (const name of names) {
    r = await admin.call("POST", "/api/admin/pins", { ttl_hours: 24 });
    const pin = r.json.pin;
    const c = new Client(base);
    r = await c.call("POST", "/api/join", { pin, username: name, password: PW });
    if (r.status !== 200) throw new Error("join failed: " + r.text);
    r = await c.call("POST", "/api/login", { username: name, password: PW });
    if (r.status !== 200) throw new Error("login failed");
    out[name] = c;
  }
  return { admin, members: out };
}

module.exports = { Client, startNode, makeMembers, PW, LUXD };
