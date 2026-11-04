// SPDX-License-Identifier: GPL-2.0-or-later
//! The web API over real HTTP: pages, sessions, CSRF defences, headers and the limiter.
use std::sync::Arc;

use lux_node::{serve, unix_now, InitParams, Node, NodeOptions, ServerHandle};
use serde_json::{json, Value};

const PW: &str = "una frase lunga e strana";

fn params(name: &str) -> InitParams {
    InitParams {
        name: name.into(),
        domain: "openpolis.site".into(),
        currency_name: "Lux".into(),
        currency_symbol: "LUX".into(),
        admin_username: "clara".into(),
        admin_password: PW.into(),
    }
}

fn start_with(name: &str, opts: NodeOptions) -> (ServerHandle, String, Arc<Node>) {
    let node = Arc::new(Node::init(":memory:", params(name), opts, unix_now()).unwrap());
    let h = serve(node.clone(), "127.0.0.1:0").unwrap();
    let base = format!("http://{}", h.addr);
    (h, base, node)
}
fn start() -> (ServerHandle, String, Arc<Node>) {
    start_with("Comunidad de prueba", NodeOptions::default())
}

struct Resp {
    status: u16,
    body: String,
    set_cookie: Option<String>,
    r: Vec<(String, String)>,
}
impl Resp {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
    fn header(&self, k: &str) -> Option<&str> {
        self.r.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str())
    }
    fn cookie(&self) -> String {
        self.set_cookie.clone().unwrap().split(';').next().unwrap().to_string()
    }
}

fn call(method: &str, url: &str, body: Option<Value>, headers: &[(&str, &str)]) -> Resp {
    let mut req = ureq::request(method, url);
    for (k, v) in headers {
        req = req.set(k, v);
    }
    let has_ct = headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type"));
    let res = match body {
        Some(b) => {
            if !has_ct {
                req = req.set("Content-Type", "application/json");
            }
            req.send_string(&b.to_string())
        }
        None => req.call(),
    };
    let r = match res {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => panic!("transport error: {e}"),
    };
    let status = r.status();
    let set_cookie = r.header("Set-Cookie").map(String::from);
    let names = r.headers_names();
    let hdrs: Vec<(String, String)> = names.iter().map(|n| (n.clone(), r.header(n).unwrap_or("").to_string())).collect();
    Resp { status, body: r.into_string().unwrap_or_default(), set_cookie, r: hdrs }
}

fn login(base: &str) -> Resp {
    call("POST", &format!("{base}/api/admin/login"), Some(json!({"username": "clara", "password": PW})), &[])
}

#[test]
fn pages_and_public_information() {
    let (h, base, _) = start();
    let r = call("GET", &format!("{base}/api/network"), None, &[]);
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["name"], "Comunidad de prueba");
    assert_eq!(r.json()["currency_symbol"], "LUX");
    let j = call("GET", &format!("{base}/join"), None, &[]);
    assert!(j.status == 200 && j.body.contains("Entra in Comunidad de prueba") && j.body.contains("/static/app.js"));
    assert!(call("GET", &format!("{base}/admin"), None, &[]).body.contains("login-form"));
    assert!(call("GET", &format!("{base}/static/app.js"), None, &[]).body.contains("/api/join"));
    assert_eq!(call("GET", &format!("{base}/nope"), None, &[]).status, 404);
    h.stop();
}

#[test]
fn security_headers_are_always_there() {
    let (h, base, _) = start();
    let r = call("GET", &format!("{base}/join"), None, &[]);
    assert!(r.header("Content-Security-Policy").unwrap().contains("script-src 'self'"));
    assert_eq!(r.header("X-Frame-Options"), Some("DENY"));
    assert_eq!(r.header("X-Content-Type-Options"), Some("nosniff"));
    assert_eq!(r.header("Cache-Control"), Some("no-store"));
    h.stop();
}

#[test]
fn the_whole_flow_over_http() {
    let (h, base, _) = start();
    // no session: no PINs
    assert_eq!(call("POST", &format!("{base}/api/admin/pins"), Some(json!({"ttl_hours": 24})), &[]).status, 401);
    assert_eq!(call("GET", &format!("{base}/api/admin/pins"), None, &[]).status, 401);
    let l = login(&base);
    assert_eq!(l.status, 200);
    let ck = l.cookie();
    let hdr = [("Cookie", ck.as_str())];
    let p = call("POST", &format!("{base}/api/admin/pins"), Some(json!({"ttl_hours": 24})), &hdr);
    assert_eq!(p.status, 200);
    let pin = p.json()["pin"].as_str().unwrap().to_string();
    assert_eq!(pin.len(), 6);
    let j = call("POST", &format!("{base}/api/join"), Some(json!({"pin": pin, "username": "Fulano", "password": PW})), &[]);
    assert_eq!((j.status, j.json()["username"].as_str()), (200, Some("fulano")));
    let again = call("POST", &format!("{base}/api/join"), Some(json!({"pin": pin, "username": "otro", "password": PW})), &[]);
    assert_eq!(again.status, 400);
    assert_eq!(again.json()["error"], "PIN non valido o scaduto");
    let list = call("GET", &format!("{base}/api/admin/pins"), None, &hdr).json();
    assert_eq!(list[0]["used_by"], "fulano");
    let members = call("GET", &format!("{base}/api/admin/members"), None, &hdr).json();
    assert_eq!(members.as_array().unwrap().len(), 2);
    // logout closes the session
    call("POST", &format!("{base}/api/admin/logout"), Some(json!({})), &hdr);
    assert_eq!(call("GET", &format!("{base}/api/admin/pins"), None, &hdr).status, 401);
    h.stop();
}

#[test]
fn cookie_flags() {
    let (h, base, _) = start();
    let c = login(&base).set_cookie.unwrap();
    assert!(c.contains("HttpOnly") && c.contains("SameSite=Strict") && c.contains("Path=/"));
    assert!(!c.contains("Secure"));
    h.stop();
    let (h, base, _) = start_with("x", NodeOptions { secure_cookies: true, ..NodeOptions::default() });
    assert!(login(&base).set_cookie.unwrap().contains("; Secure"));
    h.stop();
}

#[test]
fn cross_site_requests_are_refused() {
    let (h, base, _) = start();
    let url = format!("{base}/api/admin/login");
    let body = json!({"username": "clara", "password": PW});
    // a form posted from another site cannot send JSON, and a fetch from another origin is refused
    let r = call("POST", &url, Some(body.clone()), &[("Content-Type", "text/plain")]);
    assert_eq!(r.status, 415);
    let r = call("POST", &url, Some(body.clone()), &[("Origin", "http://evil.example")]);
    assert_eq!(r.status, 403);
    let same = format!("http://{}", h.addr);
    let r = call("POST", &url, Some(body), &[("Origin", same.as_str())]);
    assert_eq!(r.status, 200);
    let big = json!({"username": "x".repeat(20_000), "password": "y"});
    assert_eq!(call("POST", &url, Some(big), &[]).status, 400, "oversized bodies are cut and rejected");
    h.stop();
}

#[test]
fn login_failures_are_limited_over_http() {
    let (h, base, _) = start();
    let url = format!("{base}/api/admin/login");
    for _ in 0..5 {
        assert_eq!(call("POST", &url, Some(json!({"username": "clara", "password": "sbagliata sbagliata"})), &[]).status, 401);
    }
    assert_eq!(call("POST", &url, Some(json!({"username": "clara", "password": PW})), &[]).status, 429);
    h.stop();
}

#[test]
fn the_forwarded_address_is_ignored_unless_the_proxy_is_trusted() {
    let guess = |base: &str, xff: &str| {
        call("POST", &format!("{base}/api/join"), Some(json!({"pin": "000000", "username": "x1", "password": PW})), &[("X-Forwarded-For", xff)])
    };
    // not trusted: changing the header does not give the attacker fresh attempts
    let (h, base, node) = start();
    let pin = node.create_pin_local("clara", 24, unix_now()).unwrap().pin;
    let wrong = if pin == "000000" { "000001" } else { "000000" };
    for i in 0..5 {
        let r = call("POST", &format!("{base}/api/join"), Some(json!({"pin": wrong, "username": "x1", "password": PW})), &[("X-Forwarded-For", &format!("9.9.9.{i}"))]);
        assert_eq!(r.status, 400);
    }
    let r = call("POST", &format!("{base}/api/join"), Some(json!({"pin": pin, "username": "fulano", "password": PW})), &[("X-Forwarded-For", "8.8.8.8")]);
    assert_eq!(r.status, 429);
    h.stop();
    // trusted proxy: each real client address has its own attempts
    let (h, base, node) = start_with("x", NodeOptions { trust_proxy: true, ..NodeOptions::default() });
    let pin = node.create_pin_local("clara", 24, unix_now()).unwrap().pin;
    let _ = guess(&base, "7.7.7.7");
    for _ in 0..5 {
        let _ = guess(&base, "7.7.7.7");
    }
    let r = call("POST", &format!("{base}/api/join"), Some(json!({"pin": pin, "username": "fulano", "password": PW})), &[("X-Forwarded-For", "6.6.6.6")]);
    assert_eq!(r.status, 200);
    h.stop();
}

#[test]
fn the_community_name_is_escaped_in_the_pages() {
    let (h, base, _) = start_with("A<b>&\"x", NodeOptions::default());
    let body = call("GET", &format!("{base}/join"), None, &[]).body;
    assert!(body.contains("A&lt;b&gt;&amp;&quot;x"), "{body}");
    assert!(!body.contains("A<b>"));
    h.stop();
}

#[test]
fn the_member_area_over_http() {
    let (h, base, node) = start();
    let pin = node.create_pin_local("clara", 24, unix_now()).unwrap().pin;
    call("POST", &format!("{base}/api/join"), Some(json!({"pin": pin, "username": "fulano", "password": PW})), &[]);
    assert_eq!(call("GET", &format!("{base}/api/me"), None, &[]).status, 401);
    let bad = call("POST", &format!("{base}/api/login"), Some(json!({"username": "fulano", "password": "sbagliata sbagliata"})), &[]);
    assert_eq!(bad.status, 401);
    let l = call("POST", &format!("{base}/api/login"), Some(json!({"username": "fulano", "password": PW})), &[]);
    assert_eq!(l.status, 200);
    let ck = l.cookie();
    let hdr = [("Cookie", ck.as_str())];
    let me = call("GET", &format!("{base}/api/me"), None, &hdr).json();
    assert_eq!((me["username"].as_str(), me["role"].as_str()), (Some("fulano"), Some("member")));
    // a member is not an administrator
    assert_eq!(call("POST", &format!("{base}/api/admin/pins"), Some(json!({"ttl_hours": 24})), &hdr).status, 403);
    assert_eq!(call("GET", &format!("{base}/api/admin/members"), None, &hdr).status, 403);
    assert_eq!(call("POST", &format!("{base}/api/admin/login"), Some(json!({"username": "fulano", "password": PW})), &[]).status, 401);
    // pages
    assert!(call("GET", &format!("{base}/login"), None, &[]).body.contains("member-login-form"));
    assert!(call("GET", &format!("{base}/me"), None, &[]).body.contains("me-name"));
    call("POST", &format!("{base}/api/logout"), Some(json!({})), &hdr);
    assert_eq!(call("GET", &format!("{base}/api/me"), None, &hdr).status, 401);
    h.stop();
}

#[test]
fn services_over_http_and_the_proxys_certificate_question() {
    let (h, base, node) = start();
    let pin = node.create_pin_local("clara", 24, unix_now()).unwrap().pin;
    let j = call("POST", &format!("{base}/api/join"), Some(json!({"pin": pin, "username": "fulano", "password": PW, "services": ["subdomain", "mail"]})), &[]);
    assert_eq!(j.status, 200, "{}", j.body);
    let st = j.json()["services"].clone();
    assert_eq!(st.as_array().unwrap().len(), 2);
    assert_eq!(st[0]["status"], "requested", "no manager is connected to this test node");
    // the join page lists what the node offers
    let net = call("GET", &format!("{base}/api/network"), None, &[]).json();
    assert_eq!(net["services"].as_array().unwrap().len(), 4);
    // the proxy asks before asking for a certificate
    assert_eq!(call("GET", &format!("{base}/api/tls-ask?domain=fulano.openpolis.site"), None, &[]).status, 200);
    assert_eq!(call("GET", &format!("{base}/api/tls-ask?domain=evil.openpolis.site"), None, &[]).status, 404);
    assert_eq!(call("GET", &format!("{base}/api/tls-ask"), None, &[]).status, 404);
    // the member's page needs a session
    assert_eq!(call("GET", &format!("{base}/api/services"), None, &[]).status, 401);
    let l = call("POST", &format!("{base}/api/login"), Some(json!({"username": "fulano", "password": PW})), &[]);
    let ck = l.cookie();
    let hdr = [("Cookie", ck.as_str())];
    let mine = call("GET", &format!("{base}/api/services"), None, &hdr).json();
    assert_eq!(mine["mine"].as_array().unwrap().len(), 2);
    assert_eq!(call("POST", &format!("{base}/api/services"), Some(json!({"password": "sbagliata sbagliata", "services": ["chat"]})), &hdr).status, 401);
    let boh = call("POST", &format!("{base}/api/services"), Some(json!({"password": PW, "services": ["boh"]})), &hdr);
    assert_eq!(boh.status, 400, "unknown service");
    // the sub-domain was requested at sign-up, so the blog (which lives in it) can be added now
    assert_eq!(call("POST", &format!("{base}/api/services"), Some(json!({"password": PW, "services": ["insta"]})), &hdr).status, 200);
    let ok = call("POST", &format!("{base}/api/services"), Some(json!({"password": PW, "services": ["chat"]})), &hdr);
    assert_eq!(ok.status, 200, "{}", ok.body);
    assert!(call("GET", &format!("{base}/services"), None, &[]).body.contains("svc-list"));
    h.stop();
}
