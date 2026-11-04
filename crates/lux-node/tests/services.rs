// SPDX-License-Identifier: GPL-2.0-or-later
//! User services (ADR-0022): the choice at sign-up, the accounts the node asks its services to create, what is stored
//! (never the password), and which host names get a certificate.
use std::sync::Arc;

use lux_node::services::{ComposeProvisioner, RecordingRunner};
use lux_node::{InitParams, Node, NodeError, NodeOptions};

const T0: i64 = 1_800_000_000;
const PW: &str = "una frase lunga e strana";
const IP: &str = "203.0.113.7";

fn params() -> InitParams {
    InitParams {
        name: "Comunidad de prueba".into(),
        domain: "openpolis.site".into(),
        currency_name: "Lux".into(),
        currency_symbol: "LUX".into(),
        admin_username: "clara".into(),
        admin_password: PW.into(),
    }
}
fn tmp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("lux-svc-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("node.db").to_string_lossy().into_owned()
}
fn all() -> Vec<String> {
    ["subdomain", "mail", "chat", "insta"].iter().map(|s| s.to_string()).collect()
}
fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// A node whose manager records the commands instead of running them.
fn node(path: &str) -> (Node, Arc<RecordingRunner>, String) {
    let rec = Arc::new(RecordingRunner::default());
    let n = Node::init(path, params(), NodeOptions::default(), T0).unwrap().with_provisioner(Arc::new(ComposeProvisioner::new(rec.clone())));
    let admin = n.admin_login("clara", PW, IP, T0).unwrap();
    (n, rec, admin)
}
fn pin(n: &Node, admin: &str) -> String {
    n.create_pin(admin, 24, T0).unwrap().pin
}
fn commands(rec: &RecordingRunner) -> Vec<String> {
    rec.commands.lock().unwrap().iter().map(|c| c.join(" ")).collect()
}

#[test]
fn a_member_chooses_services_at_sign_up_and_the_accounts_are_created() {
    let (n, rec, admin) = node(":memory:");
    let (user, st) = n.join_with_services(&pin(&n, &admin), "fulano", PW, &all(), IP, T0).unwrap();
    assert_eq!(user.username, "fulano");
    assert_eq!(st.iter().map(|x| (x.service.as_str(), x.status.as_str())).collect::<Vec<_>>(), vec![("subdomain", "active"), ("mail", "active"), ("chat", "active"), ("insta", "active")]);
    let cmds = commands(&rec);
    let has = |needle: &str| cmds.iter().any(|c| c.contains(needle));
    assert!(has(&format!("docker exec openpolis-mail setup email add fulano@openpolis.site {PW}")), "{cmds:?}");
    assert!(has("setup quota set fulano@openpolis.site 1G"), "1 GB of mail");
    assert!(has("register_new_matrix_user") && has("-u fulano") && has("--no-admin"));
    assert!(has("wp user create fulano fulano@openpolis.site") && has("wp site create --slug=fulano"));
    assert!(has("wp option update blog_upload_space 3072 --url=fulano.openpolis.site"), "3 GB of blog");
    // the member sees where everything is
    let t = n.login("fulano", PW, IP, T0).unwrap();
    let mine = n.my_services(&t, T0).unwrap();
    let link = |svc: &str| mine.mine.iter().find(|x| x.service == svc).unwrap().link.clone();
    assert_eq!(link("subdomain").as_deref(), Some("https://fulano.openpolis.site"));
    assert_eq!(link("mail").as_deref(), Some("https://mail.openpolis.site"));
    assert_eq!(link("chat").as_deref(), Some("https://chat.openpolis.site"));
    assert_eq!(mine.offered, all());
}

#[test]
fn only_what_is_chosen_is_created_and_more_can_be_added_later() {
    let (n, rec, admin) = node(":memory:");
    let (_, st) = n.join_with_services(&pin(&n, &admin), "fulano", PW, &s(&["mail"]), IP, T0).unwrap();
    assert_eq!(st.len(), 1);
    let cmds = commands(&rec);
    assert!(cmds.iter().any(|c| c.contains("setup email add")));
    assert!(!cmds.iter().any(|c| c.contains("register_new_matrix_user") || c.contains("wp ")), "chat and blog were not asked for");
    let t = n.login("fulano", PW, IP, T0).unwrap();
    // later: the chat, but only with the password
    assert_eq!(n.request_services(&t, "password sbagliata", &s(&["chat"]), T0), Err(NodeError::Unauthorized));
    let st = n.request_services(&t, PW, &s(&["chat"]), T0).unwrap();
    assert_eq!(st.iter().map(|x| x.service.as_str()).collect::<Vec<_>>(), vec!["mail", "chat"]);
    let before = commands(&rec).len();
    n.request_services(&t, PW, &s(&["chat", "mail"]), T0).unwrap();
    assert_eq!(commands(&rec).len(), before, "what is already active is not created twice");
    // the blog needs the subdomain: asking for it alone is refused, asking for both works
    assert!(matches!(n.request_services(&t, PW, &s(&["insta"]), T0), Err(NodeError::InvalidInput(_))));
    n.request_services(&t, PW, &s(&["subdomain", "insta"]), T0).unwrap();
    assert_eq!(n.my_services(&t, T0).unwrap().mine.len(), 4);
}

#[test]
fn a_node_offers_only_the_services_its_administrator_chose() {
    let (n, _, admin) = node(":memory:");
    n.set_services(&s(&["subdomain", "mail"])).unwrap();
    assert_eq!(n.network_info().unwrap().services, s(&["subdomain", "mail"]));
    assert!(matches!(n.set_services(&s(&["insta"])), Err(NodeError::InvalidInput(_))), "the blog needs the subdomain");
    assert!(matches!(n.set_services(&s(&["boh"])), Err(NodeError::InvalidInput(_))));
    let p = pin(&n, &admin);
    assert!(matches!(n.join_with_services(&p, "fulano", PW, &s(&["chat"]), IP, T0), Err(NodeError::InvalidInput(_))), "not offered here");
    // a mistake in the choice does not burn the PIN
    assert!(n.join_with_services(&p, "fulano", PW, &s(&["mail"]), IP, T0).is_ok());
    assert!(!n.tls_allowed("chat.openpolis.site") && !n.tls_allowed("matrix.openpolis.site"));
    assert!(n.tls_allowed("mail.openpolis.site"));
}

#[test]
fn failures_are_recorded_without_the_password_and_can_be_retried() {
    let (n, rec, admin) = node(":memory:");
    rec.fail_on.lock().unwrap().push("setup email add".into());
    let (_, st) = n.join_with_services(&pin(&n, &admin), "fulano", PW, &s(&["mail", "chat"]), IP, T0).unwrap();
    let mail = st.iter().find(|x| x.service == "mail").unwrap();
    assert_eq!(mail.status, "failed");
    assert!(!mail.detail.contains("una frase lunga"), "the password must not come back: {}", mail.detail);
    assert_eq!(st.iter().find(|x| x.service == "chat").unwrap().status, "active", "one failure does not stop the others");
    assert!(mail.link.is_none());
    rec.fail_on.lock().unwrap().clear();
    let t = n.login("fulano", PW, IP, T0).unwrap();
    let st = n.request_services(&t, PW, &s(&["mail"]), T0).unwrap();
    assert_eq!(st.iter().find(|x| x.service == "mail").unwrap().status, "active");
}

#[test]
fn without_a_connected_manager_everything_stays_requested() {
    let n = Node::init(":memory:", params(), NodeOptions::default(), T0).unwrap(); // default: no manager
    let admin = n.admin_login("clara", PW, IP, T0).unwrap();
    let (_, st) = n.join_with_services(&pin(&n, &admin), "fulano", PW, &s(&["subdomain", "mail"]), IP, T0).unwrap();
    assert!(st.iter().all(|x| x.status == "requested" && x.detail.contains("in attesa")), "{st:?}");
}

#[test]
fn the_password_is_never_written_to_the_database() {
    let path = tmp("nopw");
    let (n, _, admin) = node(&path);
    let (_, _) = n.join_with_services(&pin(&n, &admin), "fulano", "una password mai salvata 42", &all(), IP, T0).unwrap();
    let t = n.login("fulano", "una password mai salvata 42", IP, T0).unwrap();
    n.request_services(&t, "una password mai salvata 42", &s(&["mail"]), T0).unwrap();
    drop(n);
    for suffix in ["", "-wal", "-shm"] {
        if let Ok(bytes) = std::fs::read(format!("{path}{suffix}")) {
            let needle = b"mai salvata";
            assert!(!bytes.windows(needle.len()).any(|w| w == needle), "the password is in {path}{suffix}");
        }
    }
}

#[test]
fn only_known_names_get_a_certificate() {
    let (n, _, admin) = node(":memory:");
    n.join_with_services(&pin(&n, &admin), "fulano", PW, &s(&["subdomain"]), IP, T0).unwrap();
    n.join_with_services(&pin(&n, &admin), "mengano", PW, &s(&["mail"]), IP, T0).unwrap(); // no sub-domain
    for (name, ok) in [
        ("openpolis.site", true),
        ("OPENPOLIS.SITE.", true),
        ("mail.openpolis.site", true),
        ("chat.openpolis.site", true),
        ("matrix.openpolis.site", true),
        ("fulano.openpolis.site", true),
        ("mengano.openpolis.site", false),
        ("nadie.openpolis.site", false),
        ("a.fulano.openpolis.site", false),
        ("fulano.evil.example", false),
        ("openpolis.site.evil.example", false),
        ("evilopenpolis.site", false),
        ("", false),
    ] {
        assert_eq!(n.tls_allowed(name), ok, "{name}");
    }
}
