// SPDX-License-Identifier: GPL-2.0-or-later
#![allow(dead_code)]
//! Shared helpers for the parliament tests: a network, members with wallets, signed payments, proposals and votes,
//! and a way to write raw entries into the ledger to prove that a tampered ledger is refused.
use ed25519_dalek::{Signer, SigningKey};
use hkdf::Hkdf;
use lux_node::{InitParams, Node, NodeError, NodeOptions, ProposalView};
use sha2::{Digest, Sha256};

pub const T0: i64 = 1_800_000_000;
pub const D: i64 = 86_400;
pub const PW: &str = "una frase lunga e strana";
pub const IP: &str = "203.0.113.7";

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
pub fn tmp(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("lux-parl-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("node.db").to_string_lossy().into_owned()
}
pub fn derive(entropy: &[u8; 16], network_id: &str) -> SigningKey {
    let hk = Hkdf::<Sha256>::new(Some(format!("openpolis/v1/{network_id}").as_bytes()), entropy);
    let mut seed = [0u8; 32];
    hk.expand(b"ed25519-signing-key", &mut seed).unwrap();
    SigningKey::from_bytes(&seed)
}

pub struct Member {
    pub name: String,
    pub token: String,
    pub key: SigningKey,
    pub next: i64,
}

pub struct Net {
    pub node: Node,
    pub admin: String,
    pub now: i64,
}

impl Net {
    pub fn on(path: &str) -> Net {
        let p = InitParams {
            name: "Comunidad de prueba".into(),
            domain: "openpolis.site".into(),
            currency_name: "Lux".into(),
            currency_symbol: "LUX".into(),
            admin_username: "clara".into(),
            admin_password: PW.into(),
        };
        let node = Node::init(path, p, NodeOptions::default(), T0).unwrap();
        let admin = node.admin_login("clara", PW, IP, T0).unwrap();
        Net { node, admin, now: T0 }
    }
    pub fn new() -> Net {
        Net::on(":memory:")
    }
    /// Joins and logs in; no wallet yet.
    pub fn join(&mut self, name: &str, seed: u8) -> Member {
        let pin = self.node.create_pin(&self.admin, 24, self.now).unwrap();
        self.node.join(&pin.pin, name, PW, IP, self.now).unwrap();
        let token = self.node.login(name, PW, IP, self.now).unwrap();
        Member { name: name.into(), token, key: derive(&[seed; 16], self.node.network_id()), next: 1 }
    }
    pub fn make_wallet(&mut self, m: &Member) {
        let pk = hex(m.key.verifying_key().as_bytes());
        let proof = hex(&m.key.sign(format!("register-key:v1:{}:{}:{pk}", self.node.network_id(), m.name).as_bytes()).to_bytes());
        self.node.register_key(&m.token, PW, &pk, &proof, self.now).unwrap();
    }
    /// Joins and creates the wallet.
    pub fn member(&mut self, name: &str, seed: u8) -> Member {
        let m = self.join(name, seed);
        self.make_wallet(&m);
        m
    }
    /// `n` members with wallets named mem1..memN.
    pub fn members(&mut self, n: u8) -> Vec<Member> {
        (1..=n).map(|i| self.member(&format!("mem{i}"), i)).collect()
    }
    pub fn advance(&mut self, days: i64) {
        self.now += days * D;
    }
    /// A new login (sessions last 12 hours).
    pub fn relogin(&self, m: &mut Member) {
        m.token = self.node.login(&m.name, PW, IP, self.now).unwrap();
    }
    fn sign(m: &Member, payload: &str) -> String {
        hex(&m.key.sign(payload.as_bytes()).to_bytes())
    }
    pub fn propose(&mut self, m: &mut Member, typ: &str, title: &str, amount: u64, to: &str, days: u64) -> Result<i64, NodeError> {
        let p = serde_json::json!({"v":1,"net":self.node.network_id(),"kind":"proposal","from":m.name,"type":typ,"title":title,"body":"testo","amount":amount,"to":to,"days":days,"nonce":m.next,"t":self.now}).to_string();
        let r = self.node.submit_proposal(&m.token, &p, &Self::sign(m, &p), self.now);
        if r.is_ok() {
            m.next += 1;
        }
        r
    }
    pub fn vote(&mut self, m: &mut Member, proposal: i64, choice: &str) -> Result<(), NodeError> {
        let p = serde_json::json!({"v":1,"net":self.node.network_id(),"kind":"vote","from":m.name,"proposal":proposal,"choice":choice,"nonce":m.next,"t":self.now}).to_string();
        let r = self.node.submit_vote(&m.token, &p, &Self::sign(m, &p), self.now);
        if r.is_ok() {
            m.next += 1;
        }
        r
    }
    pub fn pay(&mut self, m: &mut Member, to: &str, amount: u64) -> Result<lux_node::PayOutcome, NodeError> {
        let p = serde_json::json!({"v":1,"net":self.node.network_id(),"kind":"pay","from":m.name,"to":to,"amount":amount,"nonce":m.next,"memo":"","t":self.now}).to_string();
        let r = self.node.submit_pay(&m.token, &p, &Self::sign(m, &p), self.now);
        if r.is_ok() {
            m.next += 1;
        }
        r
    }
    pub fn view(&self, m: &Member, id: i64) -> ProposalView {
        self.node.proposals(&m.token, self.now).unwrap().into_iter().find(|p| p.id == id).expect("proposal")
    }
    /// Everybody in `votes` votes; the rest does not.
    pub fn vote_all(&mut self, ms: &mut [Member], id: i64, votes: &[&str]) {
        for (m, v) in ms.iter_mut().zip(votes) {
            if !v.is_empty() {
                self.vote(m, id, v).unwrap();
            }
        }
    }
    pub fn balance(&self, m: &Member) -> u64 {
        self.node.wallet(&m.token, self.now).unwrap().balance
    }
}

// ---------------------------------------------------------------- raw access to a ledger (to forge entries)
pub fn chain_hash(seq: i64, prev: &str, time: i64, kind: &str, payload: &str, signer: &str, sig: &str) -> String {
    let (s_seq, s_time) = (seq.to_string(), time.to_string());
    let mut h = Sha256::new();
    for part in [s_seq.as_str(), prev, s_time.as_str(), kind, payload, signer, sig] {
        h.update((part.len() as u64).to_le_bytes());
        h.update(part.as_bytes());
    }
    hex(&h.finalize())
}

pub fn node_key(db: &rusqlite::Connection) -> SigningKey {
    let b: Vec<u8> = db.query_row("SELECT node_key FROM network", [], |r| r.get(0)).unwrap();
    SigningKey::from_bytes(&<[u8; 32]>::try_from(b.as_slice()).unwrap())
}

/// Appends an entry at the end of the chain, with a correct hash, exactly as an attacker with the database would.
pub fn raw_append(db: &rusqlite::Connection, kind: &str, time: i64, payload: &str, signer: &str, signature: &str) {
    let (seq, prev): (i64, String) = db.query_row("SELECT seq, hash FROM ledger ORDER BY seq DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    let h = chain_hash(seq + 1, &prev, time, kind, payload, signer, signature);
    db.execute("INSERT INTO ledger (seq, prev_hash, time, kind, payload, signer, signature, hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)", rusqlite::params![seq + 1, prev, time, kind, payload, signer, signature, h]).unwrap();
}

/// A node-signed entry (the node's own key is valid: the point is what the rules accept).
pub fn raw_node_entry(db: &rusqlite::Connection, kind: &str, time: i64, payload: &str) {
    let sig = hex(&node_key(db).sign(format!("openpolis-ledger-v1\n{kind}\n{time}\n{payload}").as_bytes()).to_bytes());
    raw_append(db, kind, time, payload, "node", &sig);
}

pub fn last_time(db: &rusqlite::Connection) -> i64 {
    db.query_row("SELECT time FROM ledger ORDER BY seq DESC LIMIT 1", [], |r| r.get(0)).unwrap()
}

pub fn open_err(path: &str) -> String {
    match Node::open(path, NodeOptions::default()) {
        Ok(_) => panic!("the node opened a tampered ledger"),
        Err(e) => e.to_string(),
    }
}
