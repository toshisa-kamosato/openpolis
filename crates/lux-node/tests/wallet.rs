// SPDX-License-Identifier: GPL-2.0-or-later
//! Wallet, signed payments and the ledger (ADR-0020): what a member can do, what the node cannot do on their behalf,
//! and what happens when somebody tampers with the database.
use ed25519_dalek::{Signer, SigningKey};
use hkdf::Hkdf;
use lux_node::{InitParams, Node, NodeError, NodeOptions};
use sha2::{Digest, Sha256};

const T0: i64 = 1_800_000_000;
const D: i64 = 86_400;
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
    let dir = std::env::temp_dir().join(format!("lux-wallet-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("node.db").to_string_lossy().into_owned()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The same derivation as the browser: HKDF-SHA256, salt "openpolis/v1/<network id>", info "ed25519-signing-key".
fn derive(entropy: &[u8; 16], network_id: &str) -> SigningKey {
    let hk = Hkdf::<Sha256>::new(Some(format!("openpolis/v1/{network_id}").as_bytes()), entropy);
    let mut seed = [0u8; 32];
    hk.expand(b"ed25519-signing-key", &mut seed).unwrap();
    SigningKey::from_bytes(&seed)
}
fn payload(net: &str, from: &str, to: &str, amount: u64, nonce: i64, memo: &str) -> String {
    serde_json::json!({"v":1,"net":net,"kind":"pay","from":from,"to":to,"amount":amount,"nonce":nonce,"memo":memo,"t":T0}).to_string()
}

struct Member {
    name: String,
    token: String,
    key: SigningKey,
    next: i64,
}

struct Net {
    node: Node,
    admin: String,
    now: i64,
}

impl Net {
    fn new() -> Net {
        let node = Node::init(":memory:", params(), NodeOptions::default(), T0).unwrap();
        let admin = node.admin_login("clara", PW, IP, T0).unwrap();
        Net { node, admin, now: T0 }
    }
    fn on(path: &str) -> Net {
        let node = Node::init(path, params(), NodeOptions::default(), T0).unwrap();
        let admin = node.admin_login("clara", PW, IP, T0).unwrap();
        Net { node, admin, now: T0 }
    }
    /// A member who has joined and created a wallet.
    fn member(&mut self, name: &str, seed: u8) -> Member {
        let pin = self.node.create_pin(&self.admin, 24, self.now).unwrap();
        self.node.join(&pin.pin, name, PW, IP, self.now).unwrap();
        let token = self.node.login(name, PW, IP, self.now).unwrap();
        let key = derive(&[seed; 16], self.node.network_id());
        let pk = hex(key.verifying_key().as_bytes());
        let proof = hex(&key.sign(format!("register-key:v1:{}:{name}:{pk}", self.node.network_id()).as_bytes()).to_bytes());
        self.node.register_key(&token, PW, &pk, &proof, self.now).unwrap();
        Member { name: name.into(), token, key, next: 1 }
    }
    fn pay(&mut self, m: &mut Member, to: &str, amount: u64) -> Result<lux_node::PayOutcome, NodeError> {
        let p = payload(self.node.network_id(), &m.name, to, amount, m.next, "");
        let sig = hex(&m.key.sign(p.as_bytes()).to_bytes());
        let r = self.node.submit_pay(&m.token, &p, &sig, self.now);
        if r.is_ok() {
            m.next += 1;
        }
        r
    }
}

#[test]
fn what_the_browser_produces_verifies_here() {
    let dir = env!("CARGO_MANIFEST_DIR");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(format!("{dir}/jstests/vectors/wallet-vectors.json")).unwrap()).unwrap();
    let mut entropy = [0u8; 16];
    for i in 0..16 {
        entropy[i] = u8::from_str_radix(&v["entropy"].as_str().unwrap()[i * 2..i * 2 + 2], 16).unwrap();
    }
    // the key derived here is the one the browser derived
    let key = derive(&entropy, v["network_id"].as_str().unwrap());
    assert_eq!(hex(key.verifying_key().as_bytes()), v["public_key"].as_str().unwrap());
    // and the browser's signatures are valid for it
    let vk = key.verifying_key();
    for (msg, sig) in [(v["registration"]["message"].as_str().unwrap(), v["registration"]["signature"].as_str().unwrap()), (v["pay"]["payload"].as_str().unwrap(), v["pay"]["signature"].as_str().unwrap())] {
        let s: Vec<u8> = (0..64).map(|i| u8::from_str_radix(&sig[i * 2..i * 2 + 2], 16).unwrap()).collect();
        let sig = ed25519_dalek::Signature::from_slice(&s).unwrap();
        vk.verify_strict(msg.as_bytes(), &sig).expect("signature made in the browser");
    }
    // the 12 words follow BIP-39 (checked against the official lists with an independent implementation)
    for (lang, key) in [("english", "words_en"), ("italian", "words_it")] {
        let list = std::fs::read_to_string(format!("{dir}/static/bip39/{lang}.txt")).unwrap();
        let words: Vec<&str> = list.lines().collect();
        let mut bits: String = entropy.iter().map(|b| format!("{b:08b}")).collect();
        bits += &format!("{:08b}", Sha256::digest(entropy)[0])[..4];
        let phrase: Vec<&str> = (0..12).map(|i| words[usize::from_str_radix(&bits[i * 11..(i + 1) * 11], 2).unwrap()]).collect();
        assert_eq!(phrase.join(" "), v[key].as_str().unwrap(), "{lang}");
    }
}

#[test]
fn a_new_member_enters_the_economy_with_nothing_but_the_banks_funding() {
    let mut n = Net::new();
    let m = n.member("fulano", 1);
    let w = n.node.wallet(&m.token, n.now).unwrap();
    assert_eq!((w.balance, w.fido, w.debt, w.started), (0, 0, 0, false), "0 LUX and no credit line before the first transaction");
    assert_eq!(w.bank.members, 2);
    assert_eq!(w.bank.cash, 5_000_00 + 2 * 1_000_00, "5 000 + 1 000 for each member (the administrator and fulano)");
    assert_eq!(w.bank.emitted_total, 1_000_000_00);
    assert_eq!(w.public_key.as_deref(), Some(hex(m.key.verifying_key().as_bytes()).as_str()));
    assert_eq!(w.next_nonce, 1);
    assert_eq!(n.node.verify_ledger().unwrap(), 4, "genesis + register(clara) + register(fulano) + key(fulano)");
}

#[test]
fn a_signed_payment_moves_lux_through_the_engine() {
    let mut n = Net::new();
    let (mut a, b) = (n.member("fulano", 1), n.member("mengano", 2));
    let r = n.pay(&mut a, "mengano", 10_00).unwrap();
    assert_eq!((r.cost, r.seller_receives, r.borrowed), (10_25, 9_75, 10_25));
    let (wa, wb) = (n.node.wallet(&a.token, n.now).unwrap(), n.node.wallet(&b.token, n.now).unwrap());
    assert_eq!((wa.balance, wa.debt, wa.fido, wa.credit_available), (0, 10_25, 100_00, 89_75));
    assert_eq!((wb.balance, wb.debt, wb.fido), (9_75, 0, 100_00));
    assert_eq!(wa.next_nonce, 2);
    assert_eq!((wa.recent.len(), wa.recent[0].from.as_str(), wa.recent[0].to.as_str(), wa.recent[0].amount), (1, "fulano", "mengano", 10_00));
    assert_eq!(wb.recent.len(), 1, "the payee sees it too");
    assert_eq!(n.node.verify_ledger().unwrap(), 7);
}

#[test]
fn payments_are_checked_before_they_touch_the_ledger() {
    let mut n = Net::new();
    let (mut a, mut b) = (n.member("fulano", 1), n.member("mengano", 2));
    let net = n.node.network_id().to_string();
    let sign = |m: &Member, p: &str| hex(&m.key.sign(p.as_bytes()).to_bytes());
    let len = n.node.verify_ledger().unwrap();

    // a signature that is not the member's, or over a different text
    let p = payload(&net, "fulano", "mengano", 5_00, 1, "");
    assert!(matches!(n.node.submit_pay(&a.token, &p, &sign(&b, &p), n.now), Err(NodeError::InvalidInput(_))), "signed with someone else's key");
    assert!(matches!(n.node.submit_pay(&a.token, &p.replace("500", "900"), &sign(&a, &p), n.now), Err(NodeError::InvalidInput(_))), "text changed after signing");
    // a payment on behalf of somebody else, or for another network
    let p = payload(&net, "mengano", "fulano", 5_00, 1, "");
    assert!(matches!(n.node.submit_pay(&a.token, &p, &sign(&a, &p), n.now), Err(NodeError::InvalidInput(_))), "'from' is not the logged-in member");
    let p = payload("00000000000000000000000000000000", "fulano", "mengano", 5_00, 1, "");
    assert!(matches!(n.node.submit_pay(&a.token, &p, &sign(&a, &p), n.now), Err(NodeError::InvalidInput(_))), "signed for another network");
    // rules of the engine
    for (to, amount, what) in [("fulano", 5_00u64, "to oneself"), ("nessuno", 5_00, "to nobody"), ("mengano", 0, "zero"), ("mengano", 200_00, "beyond the credit line")] {
        let p = payload(&net, "fulano", to, amount, a.next, "");
        assert!(matches!(n.node.submit_pay(&a.token, &p, &sign(&a, &p), n.now), Err(NodeError::InvalidInput(_))), "{what}");
    }
    let long_memo = "x".repeat(81);
    let p = payload(&net, "fulano", "mengano", 5_00, a.next, &long_memo);
    assert!(matches!(n.node.submit_pay(&a.token, &p, &sign(&a, &p), n.now), Err(NodeError::InvalidInput(_))));
    assert_eq!(n.node.verify_ledger().unwrap(), len, "nothing was written");

    // replay and gaps
    n.pay(&mut a, "mengano", 5_00).unwrap();
    let again = payload(&net, "fulano", "mengano", 5_00, 1, "");
    assert!(matches!(n.node.submit_pay(&a.token, &again, &sign(&a, &again), n.now), Err(NodeError::Conflict(_))), "the same payment cannot be replayed");
    let gap = payload(&net, "fulano", "mengano", 5_00, 5, "");
    assert!(matches!(n.node.submit_pay(&a.token, &gap, &sign(&a, &gap), n.now), Err(NodeError::Conflict(_))), "nonces cannot skip");
    assert_eq!(n.node.submit_pay("token", &again, "", n.now), Err(NodeError::Unauthorized));
    // a member without a wallet cannot pay
    let pin = n.node.create_pin(&n.admin, 24, n.now).unwrap();
    n.node.join(&pin.pin, "sinllave", PW, IP, n.now).unwrap();
    let t = n.node.login("sinllave", PW, IP, n.now).unwrap();
    let p = payload(&net, "sinllave", "fulano", 1_00, 1, "");
    assert!(matches!(n.node.submit_pay(&t, &p, &sign(&b, &p), n.now), Err(NodeError::Conflict(_))));
    let _ = &mut b;
}

#[test]
fn registering_a_key_needs_the_password_and_proof_of_possession_and_happens_once() {
    let mut n = Net::new();
    let pin = n.node.create_pin(&n.admin, 24, n.now).unwrap();
    n.node.join(&pin.pin, "fulano", PW, IP, n.now).unwrap();
    let t = n.node.login("fulano", PW, IP, n.now).unwrap();
    let key = derive(&[9; 16], n.node.network_id());
    let pk = hex(key.verifying_key().as_bytes());
    let proof = hex(&key.sign(format!("register-key:v1:{}:fulano:{pk}", n.node.network_id()).as_bytes()).to_bytes());
    let other = derive(&[8; 16], n.node.network_id());
    let bad_proof = hex(&other.sign(format!("register-key:v1:{}:fulano:{pk}", n.node.network_id()).as_bytes()).to_bytes());
    let other_user = hex(&key.sign(format!("register-key:v1:{}:mengano:{pk}", n.node.network_id()).as_bytes()).to_bytes());
    assert_eq!(n.node.register_key(&t, "password sbagliata", &pk, &proof, n.now), Err(NodeError::Unauthorized));
    assert!(matches!(n.node.register_key(&t, PW, &pk, &bad_proof, n.now), Err(NodeError::InvalidInput(_))), "proof made with another key");
    assert!(matches!(n.node.register_key(&t, PW, &pk, &other_user, n.now), Err(NodeError::InvalidInput(_))), "proof made for another user name");
    assert!(matches!(n.node.register_key(&t, PW, "1234", &proof, n.now), Err(NodeError::InvalidInput(_))));
    assert_eq!(n.node.register_key("token", PW, &pk, &proof, n.now), Err(NodeError::Unauthorized));
    n.node.register_key(&t, PW, &pk, &proof, n.now).unwrap();
    assert!(matches!(n.node.register_key(&t, PW, &pk, &proof, n.now), Err(NodeError::Conflict(_))), "only once: the phrase restores it");
    // five wrong passwords lock the attempts
    let t2 = {
        let pin = n.node.create_pin(&n.admin, 24, n.now).unwrap();
        n.node.join(&pin.pin, "mengano", PW, IP, n.now).unwrap();
        n.node.login("mengano", PW, IP, n.now).unwrap()
    };
    for _ in 0..5 {
        assert_eq!(n.node.register_key(&t2, "password sbagliata", &pk, &proof, n.now), Err(NodeError::Unauthorized));
    }
    assert_eq!(n.node.register_key(&t2, PW, &pk, &proof, n.now), Err(NodeError::Throttled));
}

#[test]
fn the_economy_is_rebuilt_from_the_ledger_after_a_restart() {
    let path = tmp("restart");
    let (before, mut a) = {
        let mut n = Net::on(&path);
        let mut a = n.member("fulano", 1);
        let _b = n.member("mengano", 2);
        n.pay(&mut a, "mengano", 10_00).unwrap();
        n.pay(&mut a, "mengano", 3_00).unwrap();
        (n.node.wallet(&a.token, n.now).unwrap(), a)
    };
    let node = Node::open(&path, NodeOptions::default()).unwrap();
    let token = node.login("fulano", PW, IP, T0 + 5).unwrap();
    let after = node.wallet(&token, T0 + 5).unwrap();
    assert_eq!((after.balance, after.debt, after.fido, after.next_nonce), (before.balance, before.debt, before.fido, before.next_nonce));
    assert_eq!(after.bank.cash, before.bank.cash);
    assert_eq!(after.next_nonce, 3);
    // and it keeps working with the next nonce
    let p = payload(node.network_id(), "fulano", "mengano", 1_00, after.next_nonce, "");
    node.submit_pay(&token, &p, &hex(&a.key.sign(p.as_bytes()).to_bytes()), T0 + 6).unwrap();
    a.next = 4;
    assert_eq!(node.verify_ledger().unwrap(), 9);
}

#[test]
fn cycles_close_by_themselves_and_are_part_of_the_ledger() {
    let path = tmp("cycles");
    let mut n = Net::on(&path);
    let mut a = n.member("fulano", 1);
    let _b = n.member("mengano", 2);
    n.pay(&mut a, "mengano", 10_00).unwrap();
    assert_eq!(n.node.close_due_cycles(T0 + 27 * D).unwrap(), 0, "not yet");
    assert_eq!(n.node.close_due_cycles(T0 + 29 * D).unwrap(), 1);
    assert_eq!(n.node.close_due_cycles(T0 + 29 * D).unwrap(), 0, "once");
    assert_eq!(n.node.close_due_cycles(T0 + 60 * D).unwrap(), 1, "the second cycle (day 56)");
    let later = n.node.login("fulano", PW, IP, T0 + 60 * D).unwrap(); // sessions last 12 hours: a new login
    assert_eq!(n.node.wallet(&later, T0 + 60 * D).unwrap().bank.cycles_closed, 2);
    let n_entries = n.node.verify_ledger().unwrap();
    drop(n);
    let node = Node::open(&path, NodeOptions::default()).unwrap();
    assert_eq!(node.verify_ledger().unwrap(), n_entries);
    let t = node.login("fulano", PW, IP, T0 + 61 * D).unwrap();
    assert_eq!(node.wallet(&t, T0 + 61 * D).unwrap().bank.cycles_closed, 2, "replay gives the same state");
}

// ---------------------------------------------------------------- tampering
fn chain_hash(seq: i64, prev: &str, time: i64, kind: &str, payload: &str, signer: &str, sig: &str) -> String {
    let (s_seq, s_time) = (seq.to_string(), time.to_string());
    let mut h = Sha256::new();
    for part in [s_seq.as_str(), prev, s_time.as_str(), kind, payload, signer, sig] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part.as_bytes());
    }
    hex(&h.finalize())
}

fn scenario(name: &str) -> (String, i64) {
    let path = tmp(name);
    let mut n = Net::on(&path);
    let mut a = n.member("fulano", 1);
    let _b = n.member("mengano", 2);
    n.pay(&mut a, "mengano", 10_00).unwrap();
    n.pay(&mut a, "mengano", 3_00).unwrap();
    let len = n.node.verify_ledger().unwrap();
    (path, len)
}

fn open_err(path: &str) -> String {
    match Node::open(path, NodeOptions::default()) {
        Ok(_) => panic!("the node opened a tampered ledger"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn changing_an_entry_breaks_the_chain() {
    let (path, _) = scenario("t-change");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("UPDATE ledger SET payload = replace(payload, '\"amount\":1000', '\"amount\":10') WHERE kind = 'pay'", []).unwrap();
    assert!(open_err(&path).contains("hash non corrispondente"));
}

#[test]
fn deleting_an_entry_is_detected() {
    let (path, _) = scenario("t-delete");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute("DELETE FROM ledger WHERE seq = 5", []).unwrap();
    assert!(open_err(&path).contains("numerazione interrotta"));
}

#[test]
fn even_with_the_hashes_recomputed_a_changed_payment_has_no_valid_signature() {
    let (path, len) = scenario("t-rehash");
    let db = rusqlite::Connection::open(&path).unwrap();
    let (prev, time, kind, payload, signer, sig): (String, i64, String, String, String, String) = db
        .query_row("SELECT prev_hash, time, kind, payload, signer, signature FROM ledger WHERE seq = ?1", [len], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .unwrap();
    let forged = payload.replace("\"amount\":300", "\"amount\":9000");
    assert_ne!(forged, payload);
    let h = chain_hash(len, &prev, time, &kind, &forged, &signer, &sig);
    db.execute("UPDATE ledger SET payload = ?1, hash = ?2 WHERE seq = ?3", rusqlite::params![forged, h, len]).unwrap();
    assert!(open_err(&path).contains("firma dell'utente non valida"));
}

#[test]
fn the_node_cannot_forge_a_payment_in_a_members_name() {
    let (path, len) = scenario("t-forge");
    let db = rusqlite::Connection::open(&path).unwrap();
    let (last_hash, last_time): (String, i64) = db.query_row("SELECT hash, time FROM ledger WHERE seq = ?1", [len], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    let key_bytes: Vec<u8> = db.query_row("SELECT node_key FROM network", [], |r| r.get(0)).unwrap();
    let node_key = SigningKey::from_bytes(&<[u8; 32]>::try_from(key_bytes.as_slice()).unwrap());
    let net_id: String = {
        // the network id is public: read it from the genesis entry
        let g: String = db.query_row("SELECT payload FROM ledger WHERE seq = 1", [], |r| r.get(0)).unwrap();
        serde_json::from_str::<serde_json::Value>(&g).unwrap()["network_id"].as_str().unwrap().to_string()
    };
    // the node signs, with its own (valid) key, a payment from fulano to itself-like account mengano
    let p = payload(&net_id, "fulano", "mengano", 50_00, 3, "");
    let msg = format!("openpolis-ledger-v1\npay\n{last_time}\n{p}");
    let sig = hex(&node_key.sign(msg.as_bytes()).to_bytes());
    let h = chain_hash(len + 1, &last_hash, last_time, "pay", &p, "node", &sig);
    db.execute("INSERT INTO ledger (seq, prev_hash, time, kind, payload, signer, signature, hash) VALUES (?1,?2,?3,'pay',?4,'node',?5,?6)", rusqlite::params![len + 1, last_hash, last_time, p, sig, h]).unwrap();
    assert!(open_err(&path).contains("il firmatario non è il mittente"));
}

#[test]
fn a_pay_entry_cannot_be_moved_to_another_signer() {
    let (path, len) = scenario("t-signer");
    let db = rusqlite::Connection::open(&path).unwrap();
    let (prev, time, kind, payload, sig): (String, i64, String, String, String) = db
        .query_row("SELECT prev_hash, time, kind, payload, signature FROM ledger WHERE seq = ?1", [len], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap();
    let h = chain_hash(len, &prev, time, &kind, &payload, "user:999", &sig);
    db.execute("UPDATE ledger SET signer = 'user:999', hash = ?1 WHERE seq = ?2", rusqlite::params![h, len]).unwrap();
    assert!(open_err(&path).contains("il firmatario non è il mittente"));
}
