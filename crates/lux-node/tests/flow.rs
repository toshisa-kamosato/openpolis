// SPDX-License-Identifier: GPL-2.0-or-later
//! The M1 rules at library level: network creation, administrator, PIN invitations (ADR-0010) and sign-up.
use lux_node::{InitParams, Node, NodeError, NodeOptions};

const T0: i64 = 1_000_000;
const H: i64 = 3600;
const IP: &str = "203.0.113.7";

fn params() -> InitParams {
    InitParams {
        name: "Comunidad de prueba".into(),
        domain: "openpolis.site".into(),
        currency_name: "Lux".into(),
        currency_symbol: "LUX".into(),
        admin_username: "clara".into(),
        admin_password: "una frase lunga e strana".into(),
    }
}
fn node() -> Node {
    Node::init(":memory:", params(), NodeOptions::default(), T0).unwrap()
}
fn admin_token(n: &Node) -> String {
    n.admin_login("clara", "una frase lunga e strana", IP, T0).unwrap()
}
fn tmp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("lux-node-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("node.db").to_string_lossy().into_owned()
}

#[test]
fn init_records_the_network_and_the_first_administrator() {
    let n = node();
    let i = n.network_info().unwrap();
    assert_eq!((i.name.as_str(), i.domain.as_str()), ("Comunidad de prueba", "openpolis.site"));
    assert_eq!((i.currency_name.as_str(), i.currency_symbol.as_str(), i.members), ("Lux", "LUX", 1));
    let t = admin_token(&n);
    let me = n.session_user(&t, T0).unwrap();
    assert_eq!((me.username.as_str(), me.role.as_str()), ("clara", "admin"));
}

#[test]
fn init_refuses_bad_input_and_a_second_network() {
    let mut p = params();
    p.domain = "no valido!".into();
    assert!(matches!(Node::init(":memory:", p, NodeOptions::default(), T0), Err(NodeError::InvalidInput(_))));
    let mut p = params();
    p.currency_symbol = "TROPPO-LUNGO".into();
    assert!(matches!(Node::init(":memory:", p, NodeOptions::default(), T0), Err(NodeError::InvalidInput(_))));
    let mut p = params();
    p.admin_password = "corta".into();
    assert!(matches!(Node::init(":memory:", p, NodeOptions::default(), T0), Err(NodeError::InvalidInput(_))));
    let mut p = params();
    p.admin_username = "admin".into(); // reserved: it is a guessable name
    assert!(matches!(Node::init(":memory:", p, NodeOptions::default(), T0), Err(NodeError::InvalidInput(_))));
    let path = tmp("twice");
    Node::init(&path, params(), NodeOptions::default(), T0).unwrap();
    assert!(matches!(Node::init(&path, params(), NodeOptions::default(), T0), Err(NodeError::AlreadyInitialized)));
}

#[test]
fn admin_login_logout_and_session_expiry() {
    let n = node();
    assert_eq!(n.admin_login("clara", "sbagliata sbagliata", IP, T0), Err(NodeError::Unauthorized));
    assert_eq!(n.admin_login("nessuno", "una frase lunga e strana", IP, T0), Err(NodeError::Unauthorized), "same answer for an unknown user");
    let t = admin_token(&n);
    assert!(n.session_user(&t, T0 + 11 * H).is_some());
    assert!(n.session_user(&t, T0 + 12 * H + 1).is_none(), "sessions last 12 hours");
    let t2 = n.admin_login("CLARA", "una frase lunga e strana", IP, T0).unwrap();
    n.logout(&t2);
    assert!(n.session_user(&t2, T0).is_none());
}

#[test]
fn login_failures_are_limited_per_address() {
    let n = node();
    for _ in 0..5 {
        assert_eq!(n.admin_login("clara", "sbagliata sbagliata", IP, T0), Err(NodeError::Unauthorized));
    }
    assert_eq!(n.admin_login("clara", "una frase lunga e strana", IP, T0 + 1), Err(NodeError::Throttled), "even the right password");
    // another address, with the right password, is not affected (the per-user limit is 10 failures)
    assert!(n.admin_login("clara", "una frase lunga e strana", "198.51.100.1", T0 + 2).is_ok());
    // the lock ends after 15 minutes
    assert!(n.admin_login("clara", "una frase lunga e strana", IP, T0 + 15 * 60 + 1).is_ok());
}

#[test]
fn a_pin_has_six_digits_works_once_and_creates_a_member() {
    let n = node();
    let t = admin_token(&n);
    let pin = n.create_pin(&t, 24, T0).unwrap();
    assert_eq!(pin.pin.len(), 6);
    assert!(pin.pin.bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(pin.expires_at, T0 + 24 * H);
    let u = n.join(&pin.pin, "Fulano", "una frase lunga e strana", IP, T0 + 5).unwrap();
    assert_eq!((u.username.as_str(), u.role.as_str()), ("fulano", "member"));
    assert_eq!(n.network_info().unwrap().members, 2);
    assert_eq!(n.join(&pin.pin, "otro", "una frase lunga e strana", IP, T0 + 6), Err(NodeError::InvalidPin), "single use");
    let list = n.list_pins(&t, T0 + 7).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].used_by.as_deref(), Some("fulano"));
    assert_eq!(list[0].used_at, Some(T0 + 5));
}

#[test]
fn a_pin_works_until_it_expires_and_not_after() {
    let n = node();
    let t = admin_token(&n);
    let last_second = n.create_pin(&t, 1, T0).unwrap();
    assert!(n.join(&last_second.pin, "fulano", "una frase lunga e strana", IP, T0 + H - 1).is_ok());
    let too_late = n.create_pin(&t, 1, T0).unwrap();
    assert_eq!(n.join(&too_late.pin, "mengano", "una frase lunga e strana", IP, T0 + H + 1), Err(NodeError::InvalidPin));
}

#[test]
fn wrong_expired_used_and_malformed_pins_all_get_the_same_answer() {
    let n = node();
    let t = admin_token(&n);
    let used = n.create_pin(&t, 24, T0).unwrap();
    n.join(&used.pin, "primero", "una frase lunga e strana", IP, T0).unwrap();
    let old = n.create_pin(&t, 1, T0).unwrap();
    let wrong = if used.pin == "000000" { "000001" } else { "000000" };
    let answers: Vec<String> = [wrong, used.pin.as_str(), old.pin.as_str(), "12ab", "", "1234567"]
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let ip = format!("192.0.2.{i}"); // different addresses: this test is about the message, not the limiter
            let now = if *p == old.pin { T0 + 2 * H } else { T0 + 10 };
            n.join(p, "segundo", "una frase lunga e strana", &ip, now).unwrap_err().to_string()
        })
        .collect();
    assert!(answers.iter().all(|a| a == "PIN non valido o scaduto"), "{answers:?}");
}

#[test]
fn guessing_pins_is_limited_per_address_and_for_the_whole_node() {
    let n = node();
    let t = admin_token(&n);
    let pin = n.create_pin(&t, 24, T0).unwrap();
    let wrong = if pin.pin == "000000" { "000001" } else { "000000" };
    for _ in 0..5 {
        assert_eq!(n.join(wrong, "x1", "una frase lunga e strana", IP, T0), Err(NodeError::InvalidPin));
    }
    assert_eq!(n.join(&pin.pin, "fulano", "una frase lunga e strana", IP, T0), Err(NodeError::Throttled), "locked even for the right PIN");
    // another address is not affected...
    let n2 = node();
    let t2 = admin_token(&n2);
    let pin2 = n2.create_pin(&t2, 24, T0).unwrap();
    let wrong2 = if pin2.pin == "000000" { "000001" } else { "000000" };
    for i in 0..5 {
        let _ = n2.join(wrong2, "x", "una frase lunga e strana", IP, T0 + i);
    }
    assert!(n2.join(&pin2.pin, "fulano", "una frase lunga e strana", "198.51.100.9", T0 + 10).is_ok());
    // ...until the whole node has seen 50 failures in an hour (botnet guessing from many addresses)
    let n3 = node();
    let t3 = admin_token(&n3);
    let pin3 = n3.create_pin(&t3, 24, T0).unwrap();
    let wrong3 = if pin3.pin == "000000" { "000001" } else { "000000" };
    for i in 0..50 {
        let _ = n3.join(wrong3, "x", "una frase lunga e strana", &format!("10.0.0.{i}"), T0 + i as i64);
    }
    assert_eq!(n3.join(&pin3.pin, "fulano", "una frase lunga e strana", "10.9.9.9", T0 + 100), Err(NodeError::Throttled));
    assert!(n3.join(&pin3.pin, "fulano", "una frase lunga e strana", "10.9.9.9", T0 + H + 100).is_ok(), "the lock ends after an hour");
}

#[test]
fn a_mistake_in_the_form_does_not_burn_the_pin() {
    let n = node();
    let t = admin_token(&n);
    let pin = n.create_pin(&t, 24, T0).unwrap();
    assert!(matches!(n.join(&pin.pin, "postmaster", "una frase lunga e strana", IP, T0), Err(NodeError::InvalidInput(_))));
    assert!(matches!(n.join(&pin.pin, "fulano", "corta", IP, T0), Err(NodeError::InvalidInput(_))));
    assert!(matches!(n.join(&pin.pin, "ab", "una frase lunga e strana", IP, T0), Err(NodeError::InvalidInput(_))));
    assert!(matches!(n.join(&pin.pin, "clara", "una frase lunga e strana", IP, T0), Err(NodeError::Conflict(_))), "taken by the administrator");
    assert!(matches!(n.join(&pin.pin, "CLARA", "una frase lunga e strana", IP, T0), Err(NodeError::Conflict(_))), "case-insensitive");
    assert!(n.join(&pin.pin, "fulano", "una frase lunga e strana", IP, T0).is_ok(), "the PIN is still valid");
}

#[test]
fn only_an_administrator_session_can_issue_pins() {
    let n = node();
    let t = admin_token(&n);
    let pin = n.create_pin(&t, 24, T0).unwrap();
    n.join(&pin.pin, "fulano", "una frase lunga e strana", IP, T0).unwrap();
    assert_eq!(n.admin_login("fulano", "una frase lunga e strana", IP, T0), Err(NodeError::Unauthorized), "members have no admin login");
    assert_eq!(n.create_pin("token-inventato", 24, T0), Err(NodeError::Unauthorized));
    assert_eq!(n.list_pins("", T0), Err(NodeError::Unauthorized));
    assert_eq!(n.members("x", T0), Err(NodeError::Unauthorized));
    assert!(matches!(n.create_pin(&t, 0, T0), Err(NodeError::InvalidInput(_))));
    assert!(matches!(n.create_pin(&t, 169, T0), Err(NodeError::InvalidInput(_))));
}

#[test]
fn pins_and_passwords_are_stored_only_as_hashes() {
    let path = tmp("hashes");
    let n = Node::init(&path, params(), NodeOptions::default(), T0).unwrap();
    let t = admin_token(&n);
    let pin = n.create_pin(&t, 24, T0).unwrap();
    n.join(&pin.pin, "fulano", "una frase lunga e strana", IP, T0).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let h: String = db.query_row("SELECT pin_hmac FROM pins", [], |r| r.get(0)).unwrap();
    assert_ne!(h, pin.pin);
    assert_eq!(h.len(), 64);
    let pw: String = db.query_row("SELECT password_hash FROM users WHERE username = 'fulano'", [], |r| r.get(0)).unwrap();
    assert!(pw.starts_with("$argon2") && !pw.contains("una frase"));
    let sess: String = db.query_row("SELECT token_hash FROM sessions LIMIT 1", [], |r| r.get(0)).unwrap();
    assert_ne!(sess, t, "session tokens are stored hashed too");
}

#[test]
fn the_state_survives_a_restart() {
    let path = tmp("restart");
    let pin = {
        let n = Node::init(&path, params(), NodeOptions::default(), T0).unwrap();
        let t = admin_token(&n);
        n.create_pin(&t, 24, T0).unwrap()
    };
    let n = Node::open(&path, NodeOptions::default()).unwrap();
    assert_eq!(n.network_info().unwrap().name, "Comunidad de prueba");
    let u = n.join(&pin.pin, "fulano", "una frase lunga e strana", IP, T0 + 10).unwrap();
    assert_eq!(u.username, "fulano");
    let n = Node::open(&path, NodeOptions::default()).unwrap();
    assert_eq!(n.network_info().unwrap().members, 2);
    assert!(matches!(Node::open(&tmp("empty"), NodeOptions::default()), Err(NodeError::NotInitialized)));
}

#[test]
fn members_log_in_to_the_member_area_but_are_not_administrators() {
    let n = node();
    let t = admin_token(&n);
    let pin = n.create_pin(&t, 24, T0).unwrap();
    n.join(&pin.pin, "fulano", "una frase lunga e strana", IP, T0).unwrap();
    let m = n.login("Fulano", "una frase lunga e strana", IP, T0 + 1).unwrap();
    let me = n.session_user(&m, T0 + 1).unwrap();
    assert_eq!((me.username.as_str(), me.role.as_str()), ("fulano", "member"));
    assert_eq!(n.create_pin(&m, 24, T0 + 1), Err(NodeError::Forbidden), "a member session cannot issue PINs");
    assert_eq!(n.members(&m, T0 + 1), Err(NodeError::Forbidden));
    assert_eq!(n.login("fulano", "password sbagliata", IP, T0 + 2), Err(NodeError::Unauthorized));
    assert!(n.login("clara", "una frase lunga e strana", IP, T0 + 3).is_ok(), "the administrator is also a member");
}
