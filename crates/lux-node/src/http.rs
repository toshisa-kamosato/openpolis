// SPDX-License-Identifier: GPL-2.0-or-later
//! The local web API. Sessions live in an HttpOnly, SameSite=Strict cookie; every POST must be JSON and, when the
//! browser sends an `Origin`, it must match `Host` (so a page on another site cannot act on behalf of the admin).
use std::io::Read;
use std::net::SocketAddr;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::thread::JoinHandle;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::error::NodeError;
use crate::node::{unix_now, Node};
use crate::pages;

const COOKIE: &str = "lux_session";
const MAX_BODY: u64 = 16 * 1024;

struct Reply {
    status: u16,
    ctype: &'static str,
    body: Vec<u8>,
    extra: Vec<(&'static str, String)>,
}

impl Reply {
    fn json(status: u16, v: Value) -> Reply {
        Reply { status, ctype: "application/json; charset=utf-8", body: v.to_string().into_bytes(), extra: vec![] }
    }
    fn html(s: String) -> Reply {
        Reply { status: 200, ctype: "text/html; charset=utf-8", body: s.into_bytes(), extra: vec![] }
    }
    fn error(e: &NodeError) -> Reply {
        let (status, msg) = match e {
            NodeError::InvalidInput(_) | NodeError::InvalidPin => (400, e.to_string()),
            NodeError::Unauthorized => (401, e.to_string()),
            NodeError::Forbidden => (403, e.to_string()),
            NodeError::Throttled => (429, e.to_string()),
            NodeError::Conflict(_) | NodeError::AlreadyInitialized => (409, e.to_string()),
            NodeError::NotInitialized | NodeError::Db(_) | NodeError::LedgerCorrupt(_) => (500, "Errore interno".to_string()),
        };
        Reply::json(status, json!({ "error": msg }))
    }
    fn with(mut self, k: &'static str, v: String) -> Reply {
        self.extra.push((k, v));
        self
    }
}

pub struct ServerHandle {
    pub addr: SocketAddr,
    server: Arc<Server>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    pub fn stop(mut self) {
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
    /// Blocks until the server stops (used by `luxd run`).
    pub fn wait(mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn serve(node: Arc<Node>, addr: &str) -> std::io::Result<ServerHandle> {
    let other = |m: String| std::io::Error::new(std::io::ErrorKind::Other, m);
    let server = Arc::new(Server::http(addr).map_err(|e| other(e.to_string()))?);
    let bound = server.server_addr().to_ip().ok_or_else(|| other("no ip address".into()))?;
    let s2 = server.clone();
    let thread = std::thread::spawn(move || {
        for mut req in s2.incoming_requests() {
            let reply = catch_unwind(AssertUnwindSafe(|| route(&node, &mut req)))
                .unwrap_or_else(|_| Reply::json(500, json!({ "error": "Errore interno" })));
            let _ = respond(req, reply);
        }
    });
    Ok(ServerHandle { addr: bound, server, thread: Some(thread) })
}

fn respond(req: Request, r: Reply) -> std::io::Result<()> {
    let hdr = |k: &str, v: &str| Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("valid header");
    let mut resp = Response::from_data(r.body).with_status_code(r.status);
    resp.add_header(hdr("Content-Type", r.ctype));
    resp.add_header(hdr("Cache-Control", "no-store"));
    resp.add_header(hdr("X-Content-Type-Options", "nosniff"));
    resp.add_header(hdr("X-Frame-Options", "DENY"));
    resp.add_header(hdr("Referrer-Policy", "no-referrer"));
    resp.add_header(hdr(
        "Content-Security-Policy",
        "default-src 'self'; style-src 'unsafe-inline'; script-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
    ));
    for (k, v) in &r.extra {
        resp.add_header(hdr(k, v));
    }
    req.respond(resp)
}

fn header(req: &Request, name: &'static str) -> Option<String> {
    req.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_string())
}

fn client_ip(node: &Node, req: &Request) -> String {
    if node.opts.trust_proxy {
        if let Some(x) = header(req, "X-Forwarded-For") {
            if let Some(first) = x.split(',').next() {
                let t = first.trim();
                if !t.is_empty() {
                    return t.to_string();
                }
            }
        }
    }
    req.remote_addr().map(|a| a.ip().to_string()).unwrap_or_else(|| "unknown".into())
}

fn session_token(req: &Request) -> Option<String> {
    let c = header(req, "Cookie")?;
    c.split(';').filter_map(|p| p.trim().strip_prefix("lux_session=")).next().map(|s| s.to_string())
}

fn cookie(node: &Node, token: &str, max_age: i64) -> String {
    let secure = if node.opts.secure_cookies { "; Secure" } else { "" };
    format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}")
}

fn check_post(req: &Request) -> Result<(), Reply> {
    let ct = header(req, "Content-Type").unwrap_or_default().to_lowercase();
    if !ct.starts_with("application/json") {
        return Err(Reply::json(415, json!({ "error": "Content-Type deve essere application/json" })));
    }
    if let (Some(origin), Some(host)) = (header(req, "Origin"), header(req, "Host")) {
        if origin.split("://").nth(1).unwrap_or("") != host {
            return Err(Reply::json(403, json!({ "error": "Origine non consentita" })));
        }
    }
    Ok(())
}

fn read_json(req: &mut Request) -> Result<Value, Reply> {
    let mut buf = String::new();
    req.as_reader().take(MAX_BODY).read_to_string(&mut buf).map_err(|_| Reply::json(400, json!({ "error": "Richiesta non valida" })))?;
    serde_json::from_str(&buf).map_err(|_| Reply::json(400, json!({ "error": "JSON non valido" })))
}

fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn js(src: &'static str) -> Reply {
    Reply { status: 200, ctype: "application/javascript; charset=utf-8", body: src.as_bytes().to_vec(), extra: vec![] }
}

fn route(node: &Node, req: &mut Request) -> Reply {
    let now = unix_now();
    let method = req.method().clone();
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();
    let ip = client_ip(node, req);
    let net = |f: fn(&crate::node::NetworkInfo) -> String| match node.network_info() {
        Ok(n) => Reply::html(f(&n)),
        Err(e) => Reply::error(&e),
    };
    match (&method, path.as_str()) {
        (Method::Get, "/") => net(pages::index),
        (Method::Get, "/join") => net(pages::join),
        (Method::Get, "/admin") => net(pages::admin),
        (Method::Get, "/login") => net(pages::login),
        (Method::Get, "/me") => net(pages::me),
        (Method::Get, "/wallet") => net(pages::wallet),
        (Method::Get, "/parliament") => net(pages::parliament),
        (Method::Get, "/static/parliament.js") => js(include_str!("../static/parliament.js")),
        (Method::Get, "/static/app.js") => js(pages::APP_JS),
        (Method::Get, "/static/wallet.js") => js(include_str!("../static/wallet.js")),
        (Method::Get, "/static/wallet-core.js") => js(include_str!("../static/wallet-core.js")),
        (Method::Get, "/static/wordlists.js") => js(include_str!("../static/wordlists.js")),
        (Method::Get, "/api/network") => match node.network_info() {
            Ok(n) => Reply::json(200, serde_json::to_value(n).unwrap_or(Value::Null)),
            Err(e) => Reply::error(&e),
        },
        (Method::Post, p) if p.starts_with("/api/") => {
            if let Err(r) = check_post(req) {
                return r;
            }
            let body = match read_json(req) {
                Ok(b) => b,
                Err(r) => return r,
            };
            match p {
                "/api/admin/login" => match node.admin_login(text(&body, "username"), text(&body, "password"), &ip, now) {
                    Ok(t) => Reply::json(200, json!({ "ok": true })).with("Set-Cookie", cookie(node, &t, node.opts.session_ttl_secs)),
                    Err(e) => Reply::error(&e),
                },
                "/api/login" => match node.login(text(&body, "username"), text(&body, "password"), &ip, now) {
                    Ok(t) => Reply::json(200, json!({ "ok": true })).with("Set-Cookie", cookie(node, &t, node.opts.session_ttl_secs)),
                    Err(e) => Reply::error(&e),
                },
                "/api/me/key" => {
                    let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
                    match node.register_key(&t, text(&body, "password"), text(&body, "public_key"), text(&body, "signature"), now) {
                        Ok(()) => Reply::json(200, json!({ "ok": true })),
                        Err(e) => Reply::error(&e),
                    }
                }
                "/api/pay" => {
                    let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
                    match node.submit_pay(&t, text(&body, "payload"), text(&body, "signature"), now) {
                        Ok(o) => Reply::json(200, serde_json::to_value(o).unwrap_or(Value::Null)),
                        Err(e) => Reply::error(&e),
                    }
                }
                "/api/proposals" => {
                    let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
                    match node.submit_proposal(&t, text(&body, "payload"), text(&body, "signature"), now) {
                        Ok(id) => Reply::json(200, json!({ "ok": true, "id": id })),
                        Err(e) => Reply::error(&e),
                    }
                }
                "/api/vote" => {
                    let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
                    match node.submit_vote(&t, text(&body, "payload"), text(&body, "signature"), now) {
                        Ok(()) => Reply::json(200, json!({ "ok": true })),
                        Err(e) => Reply::error(&e),
                    }
                }
                "/api/admin/logout" | "/api/logout" => {
                    if let Some(t) = session_token(req) {
                        node.logout(&t);
                    }
                    Reply::json(200, json!({ "ok": true })).with("Set-Cookie", cookie(node, "", 0))
                }
                "/api/admin/pins" => {
                    let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
                    let ttl = body.get("ttl_hours").and_then(Value::as_i64).unwrap_or(24);
                    match node.create_pin(&t, ttl, now) {
                        Ok(p) => Reply::json(200, serde_json::to_value(p).unwrap_or(Value::Null)),
                        Err(e) => Reply::error(&e),
                    }
                }
                "/api/join" => {
                    let wanted: Vec<String> = body.get("services").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                    match node.join_with_services(text(&body, "pin"), text(&body, "username"), text(&body, "password"), &wanted, &ip, now) {
                        Ok((u, st)) => Reply::json(200, json!({ "ok": true, "username": u.username, "services": st })),
                        Err(e) => Reply::error(&e),
                    }
                }
                "/api/services" => {
                    let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
                    let wanted: Vec<String> = body.get("services").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
                    match node.request_services(&t, text(&body, "password"), &wanted, now) {
                        Ok(st) => Reply::json(200, json!({ "ok": true, "services": st })),
                        Err(e) => Reply::error(&e),
                    }
                }
                _ => Reply::json(404, json!({ "error": "Non trovato" })),
            }
        }
        (Method::Get, "/api/wallet") => {
            let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
            match node.wallet(&t, now) {
                Ok(w) => Reply::json(200, serde_json::to_value(w).unwrap_or(Value::Null)),
                Err(e) => Reply::error(&e),
            }
        }
        (Method::Get, "/api/services") => {
            let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
            match node.my_services(&t, now) {
                Ok(s) => Reply::json(200, serde_json::to_value(s).unwrap_or(Value::Null)),
                Err(e) => Reply::error(&e),
            }
        }
        (Method::Get, "/api/tls-ask") => {
            // the proxy's question before asking for a certificate: 200 = this name is ours
            let domain = url.split('?').nth(1).unwrap_or("").split('&').find_map(|kv| kv.strip_prefix("domain=")).unwrap_or("");
            if node.tls_allowed(domain) { Reply::json(200, json!({ "ok": true })) } else { Reply::json(404, json!({ "error": "nome sconosciuto" })) }
        }
        (Method::Get, "/services") => net(pages::services),
        (Method::Get, "/.well-known/matrix/server") | (Method::Get, "/.well-known/matrix/client") => match node.network_info() {
            Ok(n) if n.services.iter().any(|s| s == "chat") => {
                let body = if path.ends_with("/server") { json!({ "m.server": format!("matrix.{}:443", n.domain) }) } else { json!({ "m.homeserver": { "base_url": format!("https://matrix.{}", n.domain) } }) };
                Reply::json(200, body).with("Access-Control-Allow-Origin", "*".into())
            }
            _ => Reply::json(404, json!({ "error": "Non trovato" })),
        },
        (Method::Get, "/api/proposals") => {
            let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
            match node.proposals(&t, now) {
                Ok(l) => Reply::json(200, serde_json::to_value(l).unwrap_or(Value::Null)),
                Err(e) => Reply::error(&e),
            }
        }
        (Method::Get, "/api/me") => {
            let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
            match node.session_user(&t, now) {
                Some(u) => Reply::json(200, serde_json::to_value(u).unwrap_or(Value::Null)),
                None => Reply::error(&NodeError::Unauthorized),
            }
        }
        (Method::Get, "/api/admin/pins") => {
            let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
            match node.list_pins(&t, now) {
                Ok(l) => Reply::json(200, serde_json::to_value(l).unwrap_or(Value::Null)),
                Err(e) => Reply::error(&e),
            }
        }
        (Method::Get, "/api/admin/members") => {
            let Some(t) = session_token(req) else { return Reply::error(&NodeError::Unauthorized) };
            match node.members(&t, now) {
                Ok(l) => Reply::json(200, serde_json::to_value(l).unwrap_or(Value::Null)),
                Err(e) => Reply::error(&e),
            }
        }
        _ => Reply::json(404, json!({ "error": "Non trovato" })),
    }
}
