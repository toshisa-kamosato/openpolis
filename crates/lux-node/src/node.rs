// SPDX-License-Identifier: GPL-2.0-or-later
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use lux_economy::{Economy, Params};
use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};

use crate::auth::*;
use crate::error::NodeError;
use crate::ledger::{self, Applied, ApplyError, ErrKind, ParliamentParams, State, Status};
use crate::services::{self, NoopProvisioner, Outcome, ProvisionRequest, Provisioner};

pub fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS network (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    name TEXT NOT NULL, domain TEXT NOT NULL,
    currency_name TEXT NOT NULL, currency_symbol TEXT NOT NULL,
    created_at INTEGER NOT NULL, secret BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin','member')),
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pins (
    id INTEGER PRIMARY KEY,
    pin_hmac TEXT NOT NULL UNIQUE,
    created_by INTEGER NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
    used_at INTEGER, used_by INTEGER REFERENCES users(id)
);
CREATE TABLE IF NOT EXISTS throttle (
    key TEXT PRIMARY KEY, count INTEGER NOT NULL, reset_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS member_services (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    service TEXT NOT NULL, status TEXT NOT NULL, detail TEXT NOT NULL, updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, service)
);
CREATE TABLE IF NOT EXISTS ledger (
    seq INTEGER PRIMARY KEY, prev_hash TEXT NOT NULL, time INTEGER NOT NULL, kind TEXT NOT NULL,
    payload TEXT NOT NULL, signer TEXT NOT NULL, signature TEXT NOT NULL, hash TEXT NOT NULL UNIQUE
);";

const LOGIN_IP: (i64, i64) = (5, 15 * 60);
const LOGIN_USER: (i64, i64) = (10, 15 * 60);
const PIN_IP: (i64, i64) = (5, 15 * 60);
const PIN_ALL: (i64, i64) = (50, 60 * 60);
pub const MAX_PIN_TTL_HOURS: i64 = 7 * 24;

#[derive(Clone, Debug)]
pub struct NodeOptions {
    pub session_ttl_secs: i64,
    /// Read the client address from `X-Forwarded-For` (only behind a proxy you trust).
    pub trust_proxy: bool,
    pub secure_cookies: bool,
}

impl Default for NodeOptions {
    fn default() -> Self {
        NodeOptions { session_ttl_secs: 12 * 3600, trust_proxy: false, secure_cookies: false }
    }
}

#[derive(Clone, Debug)]
pub struct InitParams {
    pub name: String,
    pub domain: String,
    pub currency_name: String,
    pub currency_symbol: String,
    pub admin_username: String,
    pub admin_password: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct NetworkInfo {
    pub network_id: String,
    pub name: String,
    pub domain: String,
    pub currency_name: String,
    pub currency_symbol: String,
    pub created_at: i64,
    pub members: i64,
    /// Services this node offers to its members (a choice of the node's administrator).
    pub services: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct UserInfo {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct NewPin {
    pub pin: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PinInfo {
    pub id: i64,
    pub created_at: i64,
    pub expires_at: i64,
    pub used_at: Option<i64>,
    pub used_by: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BankView {
    pub cash: u64,
    pub required: u64,
    pub surplus: i64,
    pub advance_outstanding: u64,
    pub mint_balance: u64,
    pub emitted_total: u64,
    pub members: u64,
    pub cycles_closed: u64,
    pub next_cycle_at: u64,
    pub loss_target: u64,
    pub loss_held: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LedgerLine {
    pub seq: i64,
    pub time: i64,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub memo: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WalletView {
    pub username: String,
    pub network_id: String,
    pub currency_symbol: String,
    /// Registered public key (hex), if the member has created a wallet.
    pub public_key: Option<String>,
    pub next_nonce: i64,
    /// All amounts in centi-LUX.
    pub balance: u64,
    pub debt: u64,
    pub fido: u64,
    pub credit_available: u64,
    pub blocked: bool,
    pub started: bool,
    pub bank: BankView,
    pub recent: Vec<LedgerLine>,
}

#[derive(Clone, Debug, Serialize)]
pub struct VoteLine {
    pub user: String,
    pub choice: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProposalView {
    pub id: i64,
    /// "text", "mint", "works" or "expense"
    pub kind: String,
    pub title: String,
    pub body: String,
    pub proposer: String,
    /// Centi-LUX (0 for a text proposal).
    pub amount: u64,
    pub to: Option<String>,
    pub created: i64,
    pub deadline: i64,
    /// "open", "accepted", "rejected", "no_quorum" or "void"
    pub status: String,
    /// "simple" (more yes than no) or "two_thirds" (an emission of money)
    pub needs: String,
    pub yes: u64,
    pub no: u64,
    pub abstain: u64,
    pub eligible: u64,
    pub quorum_needed: u64,
    /// Open ballot: everybody can see how each member voted.
    pub votes: Vec<VoteLine>,
    pub my_vote: Option<String>,
    pub can_vote: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PayOutcome {
    pub seq: i64,
    pub cost: u64,
    pub seller_receives: u64,
    pub fee_buyer: u64,
    pub fee_seller: u64,
    pub borrowed: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ServiceStatus {
    pub service: String,
    pub label: String,
    /// "requested", "active" or "failed"
    pub status: String,
    pub detail: String,
    pub link: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MyServices {
    pub offered: Vec<String>,
    pub mine: Vec<ServiceStatus>,
}

pub struct Node {
    provisioner: Arc<dyn Provisioner>,
    conn: Mutex<Connection>,
    secret: Vec<u8>,
    pub opts: NodeOptions,
    node_key: SigningKey,
    network_id: String,
    state: Mutex<State>,
}

fn migrate(c: &Connection) -> Result<(), NodeError> {
    c.execute_batch(SCHEMA)?;
    for stmt in ["ALTER TABLE network ADD COLUMN node_key BLOB", "ALTER TABLE network ADD COLUMN services TEXT"] {
        match c.execute(stmt, []) {
            Ok(_) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn derive_network_id(secret: &[u8]) -> String {
    hmac_hex(secret, "network-id")[..32].to_string()
}

fn node_entry(c: &Connection, key: &SigningKey, kind: &str, time: i64, payload: &Value) -> Result<(), NodeError> {
    let p = payload.to_string();
    let sig = hex(&key.sign(&ledger::node_message(kind, time, &p)).to_bytes());
    ledger::append(c, kind, time, &p, "node", &sig)?;
    Ok(())
}

/// "fondi insufficienti" -> "Fondi insufficienti."
fn say(m: &str) -> String {
    let mut c = m.chars();
    let s: String = match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    };
    if s.ends_with('.') { s } else { s + "." }
}

fn apply_to_node(e: ApplyError) -> NodeError {
    match e.kind {
        ErrKind::Invalid => NodeError::InvalidInput(say(&e.msg)),
        ErrKind::Conflict => NodeError::Conflict(say(&e.msg)),
    }
}

fn open_conn(path: &str) -> Result<Connection, NodeError> {
    let c = Connection::open(path)?;
    c.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    if path != ":memory:" {
        let _: String = c.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    }
    Ok(c)
}

fn is_hostname(d: &str) -> bool {
    !d.is_empty()
        && d.len() <= 253
        && d.split('.').all(|l| {
            !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

fn is_constraint(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation)
}

fn user_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserInfo> {
    Ok(UserInfo { id: r.get(0)?, username: r.get(1)?, role: r.get(2)?, created_at: r.get(3)? })
}

impl Node {
    /// Creates the network on an empty database: name, domain, currency and the first administrator.
    pub fn init(path: &str, p: InitParams, opts: NodeOptions, now: i64) -> Result<Node, NodeError> {
        let name = p.name.trim().to_string();
        if name.is_empty() || name.chars().count() > 60 || name.chars().any(|c| c.is_control()) {
            return Err(NodeError::InvalidInput("Il nome della comunità deve avere da 1 a 60 caratteri.".into()));
        }
        let domain = p.domain.trim().to_lowercase();
        if !is_hostname(&domain) {
            return Err(NodeError::InvalidInput("Il dominio non è valido (esempio: openpolis.site).".into()));
        }
        let (cname, csym) = (p.currency_name.trim().to_string(), p.currency_symbol.trim().to_string());
        if cname.is_empty() || cname.chars().count() > 30 || cname.chars().any(|c| c.is_control()) {
            return Err(NodeError::InvalidInput("Il nome della moneta deve avere da 1 a 30 caratteri.".into()));
        }
        if csym.is_empty() || csym.len() > 8 || !csym.chars().all(|c| c.is_ascii_alphanumeric() || c == '$') {
            return Err(NodeError::InvalidInput("Il simbolo della moneta: da 1 a 8 lettere, numeri o «$».".into()));
        }
        let admin = validate_username(&p.admin_username)?;
        validate_password(&p.admin_password, &admin)?;

        let mut conn = open_conn(path)?;
        migrate(&conn)?;
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM network", [], |r| r.get(0))?;
        if n > 0 {
            return Err(NodeError::AlreadyInitialized);
        }
        let secret = random_bytes::<32>();
        let hash = hash_password(&p.admin_password)?;
        let node_key = SigningKey::from_bytes(&random_bytes::<32>());
        let network_id = derive_network_id(&secret);
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO network (id, name, domain, currency_name, currency_symbol, created_at, secret, node_key)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![name, domain, cname, csym, now, secret.to_vec(), node_key.to_bytes().to_vec()],
        )?;
        tx.execute(
            "INSERT INTO users (username, password_hash, role, created_at) VALUES (?1, ?2, 'admin', ?3)",
            params![admin, hash, now],
        )?;
        let admin_id = tx.last_insert_rowid();
        node_entry(&tx, &node_key, "genesis", now, &json!({ "params": Params::default(), "parliament": ParliamentParams::default(), "network_id": network_id }))?;
        node_entry(&tx, &node_key, "register", now, &json!({ "account": admin_id, "username": admin }))?;
        let state = ledger::replay(&ledger::all(&tx)?, &node_key.verifying_key(), &network_id)?.expect("genesis was just written");
        tx.commit()?;
        Ok(Node { provisioner: Arc::new(NoopProvisioner), conn: Mutex::new(conn), secret: secret.to_vec(), opts, node_key, network_id, state: Mutex::new(state) })
    }

    /// Opens an existing node and rebuilds the economic state by replaying (and verifying) the whole ledger.
    pub fn open(path: &str, opts: NodeOptions) -> Result<Node, NodeError> {
        let mut conn = open_conn(path)?;
        migrate(&conn)?;
        let row: Option<(Vec<u8>, Option<Vec<u8>>, i64)> = conn
            .query_row("SELECT secret, node_key, created_at FROM network WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?;
        let (secret, key_bytes, created_at) = row.ok_or(NodeError::NotInitialized)?;
        let node_key = match key_bytes.as_deref().map(<[u8; 32]>::try_from) {
            Some(Ok(b)) => SigningKey::from_bytes(&b),
            _ => {
                let k = SigningKey::from_bytes(&random_bytes::<32>());
                conn.execute("UPDATE network SET node_key = ?1 WHERE id = 1", params![k.to_bytes().to_vec()])?;
                k
            }
        };
        let network_id = derive_network_id(&secret);
        if ledger::last(&conn)?.is_none() {
            // a database created before the ledger existed: start the ledger and register the existing members
            let tx = conn.transaction()?;
            let t = unix_now().max(created_at);
            node_entry(&tx, &node_key, "genesis", t, &json!({ "params": Params::default(), "parliament": ParliamentParams::default(), "network_id": network_id }))?;
            let users: Vec<(i64, String)> = {
                let mut st = tx.prepare("SELECT id, username FROM users ORDER BY id")?;
                let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            for (id, name) in users {
                node_entry(&tx, &node_key, "register", t, &json!({ "account": id, "username": name }))?;
            }
            tx.commit()?;
        }
        let state = ledger::replay(&ledger::all(&conn)?, &node_key.verifying_key(), &network_id)?.ok_or(NodeError::NotInitialized)?;
        Ok(Node { provisioner: Arc::new(NoopProvisioner), conn: Mutex::new(conn), secret, opts, node_key, network_id, state: Mutex::new(state) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn network_info(&self) -> Result<NetworkInfo, NodeError> {
        let c = self.lock();
        let members: i64 = c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        let services = Self::offered_in(&c)?;
        Ok(c.query_row(
            "SELECT name, domain, currency_name, currency_symbol, created_at FROM network WHERE id = 1",
            [],
            |r| {
                Ok(NetworkInfo {
                    network_id: self.network_id.clone(),
                    name: r.get(0)?,
                    domain: r.get(1)?,
                    currency_name: r.get(2)?,
                    currency_symbol: r.get(3)?,
                    created_at: r.get(4)?,
                    members,
                    services: services.clone(),
                })
            },
        )?)
    }

    // ------------------------------------------------------------------ sessions
    /// Administrator login (the admin page). Returns the session token (shown once, stored only as a hash).
    pub fn admin_login(&self, username: &str, password: &str, ip: &str, now: i64) -> Result<String, NodeError> {
        self.login_inner(username, password, ip, now, true)
    }

    /// Login of any member (administrators included): the member area.
    pub fn login(&self, username: &str, password: &str, ip: &str, now: i64) -> Result<String, NodeError> {
        self.login_inner(username, password, ip, now, false)
    }

    fn login_inner(&self, username: &str, password: &str, ip: &str, now: i64, admin_only: bool) -> Result<String, NodeError> {
        let u = username.trim().to_lowercase();
        let (ip_key, user_key) = (format!("login:ip:{ip}"), format!("login:user:{u}"));
        let c = self.lock();
        if is_locked(&c, &ip_key, LOGIN_IP.0, now)? || is_locked(&c, &user_key, LOGIN_USER.0, now)? {
            return Err(NodeError::Throttled);
        }
        let row: Option<(i64, String, String)> = c
            .query_row("SELECT id, password_hash, role FROM users WHERE username = ?1", params![u], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let ok_user = match &row {
            Some((id, hash, role)) if verify_password(hash, password) && (!admin_only || role == "admin") => Some(*id),
            Some(_) => None,
            None => {
                dummy_verify(password); // same cost whether or not the user exists
                None
            }
        };
        let Some(uid) = ok_user else {
            record_failure(&c, &ip_key, LOGIN_IP.1, now)?;
            record_failure(&c, &user_key, LOGIN_USER.1, now)?;
            return Err(NodeError::Unauthorized);
        };
        let token = new_token();
        c.execute(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
            params![sha256_hex(&token), uid, now, now + self.opts.session_ttl_secs],
        )?;
        c.execute("DELETE FROM sessions WHERE expires_at <= ?1", params![now])?;
        Ok(token)
    }

    pub fn logout(&self, token: &str) {
        let _ = self.lock().execute("DELETE FROM sessions WHERE token_hash = ?1", params![sha256_hex(token)]);
    }

    pub fn session_user(&self, token: &str, now: i64) -> Option<UserInfo> {
        let c = self.lock();
        Self::session_user_in(&c, token, now)
    }

    fn session_user_in(c: &Connection, token: &str, now: i64) -> Option<UserInfo> {
        c.query_row(
            "SELECT u.id, u.username, u.role, u.created_at FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ?1 AND s.expires_at > ?2",
            params![sha256_hex(token), now],
            user_from_row,
        )
        .optional()
        .ok()
        .flatten()
    }

    fn require_admin(c: &Connection, token: &str, now: i64) -> Result<UserInfo, NodeError> {
        let u = Self::session_user_in(c, token, now).ok_or(NodeError::Unauthorized)?;
        if u.role != "admin" {
            return Err(NodeError::Forbidden);
        }
        Ok(u)
    }

    // ------------------------------------------------------------------ PINs
    fn issue_pin(&self, c: &Connection, admin_id: i64, ttl_hours: i64, now: i64) -> Result<NewPin, NodeError> {
        if !(1..=MAX_PIN_TTL_HOURS).contains(&ttl_hours) {
            return Err(NodeError::InvalidInput(format!("La durata deve essere da 1 a {MAX_PIN_TTL_HOURS} ore.")));
        }
        let expires_at = now + ttl_hours * 3600;
        for _ in 0..50 {
            let pin = random_pin();
            let res = c.execute(
                "INSERT INTO pins (pin_hmac, created_by, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
                params![hmac_hex(&self.secret, &pin), admin_id, now, expires_at],
            );
            match res {
                Ok(_) => return Ok(NewPin { pin, expires_at }),
                Err(e) if is_constraint(&e) => continue, // same digits as an old PIN: draw again
                Err(e) => return Err(e.into()),
            }
        }
        Err(NodeError::Db("cannot find a free PIN".into()))
    }

    /// New invitation PIN (admin session). The PIN is returned once: only its HMAC is stored.
    pub fn create_pin(&self, token: &str, ttl_hours: i64, now: i64) -> Result<NewPin, NodeError> {
        let c = self.lock();
        let admin = Self::require_admin(&c, token, now)?;
        self.issue_pin(&c, admin.id, ttl_hours, now)
    }

    /// New PIN issued by whoever operates the machine (`luxd pin new`), on behalf of an administrator.
    pub fn create_pin_local(&self, admin_username: &str, ttl_hours: i64, now: i64) -> Result<NewPin, NodeError> {
        let c = self.lock();
        let id: Option<i64> = c
            .query_row(
                "SELECT id FROM users WHERE username = ?1 AND role = 'admin'",
                params![admin_username.trim().to_lowercase()],
                |r| r.get(0),
            )
            .optional()?;
        let id = id.ok_or_else(|| NodeError::InvalidInput("Amministratore inesistente.".into()))?;
        self.issue_pin(&c, id, ttl_hours, now)
    }

    pub fn list_pins(&self, token: &str, now: i64) -> Result<Vec<PinInfo>, NodeError> {
        let c = self.lock();
        Self::require_admin(&c, token, now)?;
        let mut st = c.prepare(
            "SELECT p.id, p.created_at, p.expires_at, p.used_at, u.username
             FROM pins p LEFT JOIN users u ON u.id = p.used_by ORDER BY p.id DESC LIMIT 200",
        )?;
        let rows = st.query_map([], |r| {
            Ok(PinInfo { id: r.get(0)?, created_at: r.get(1)?, expires_at: r.get(2)?, used_at: r.get(3)?, used_by: r.get(4)? })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn members(&self, token: &str, now: i64) -> Result<Vec<UserInfo>, NodeError> {
        let c = self.lock();
        Self::require_admin(&c, token, now)?;
        let mut st = c.prepare("SELECT id, username, role, created_at FROM users ORDER BY id")?;
        let rows = st.query_map([], user_from_row)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    // ------------------------------------------------------------------ sign-up
    /// Creates a member with a PIN. Wrong, expired and used PINs give the same answer, and failed attempts are
    /// limited per address (5 / 15 min) and for the whole node (50 / hour).
    pub fn join(&self, pin: &str, username: &str, password: &str, ip: &str, now: i64) -> Result<UserInfo, NodeError> {
        let (ip_key, all_key) = (format!("pin:ip:{ip}"), "pin:all".to_string());
        let mut c = self.lock();
        if is_locked(&c, &ip_key, PIN_IP.0, now)? || is_locked(&c, &all_key, PIN_ALL.0, now)? {
            return Err(NodeError::Throttled);
        }
        let pin = pin.trim();
        let pin_id: Option<i64> = if valid_pin_format(pin) {
            c.query_row(
                "SELECT id FROM pins WHERE pin_hmac = ?1 AND used_at IS NULL AND expires_at > ?2",
                params![hmac_hex(&self.secret, pin), now],
                |r| r.get(0),
            )
            .optional()?
        } else {
            None
        };
        let Some(pin_id) = pin_id else {
            record_failure(&c, &ip_key, PIN_IP.1, now)?;
            record_failure(&c, &all_key, PIN_ALL.1, now)?;
            return Err(NodeError::InvalidPin);
        };
        // the PIN is valid: now the data are checked (a mistake here does not burn the PIN)
        let username = validate_username(username)?;
        validate_password(password, &username)?;
        let hash = hash_password(password)?;
        let tx = c.transaction()?;
        match tx.execute(
            "INSERT INTO users (username, password_hash, role, created_at) VALUES (?1, ?2, 'member', ?3)",
            params![username, hash, now],
        ) {
            Ok(_) => {}
            Err(e) if is_constraint(&e) => return Err(NodeError::Conflict("Questo nome utente è già in uso.".into())),
            Err(e) => return Err(e.into()),
        }
        let uid = tx.last_insert_rowid();
        let used = tx.execute("UPDATE pins SET used_at = ?1, used_by = ?2 WHERE id = ?3 AND used_at IS NULL", params![now, uid, pin_id])?;
        if used != 1 {
            return Err(NodeError::InvalidPin); // someone else used it in the meantime; the transaction rolls back
        }
        // the new member enters the economy: the mint funds the common cash (it can run out, see ADR-0019)
        let mut st = self.lock_state();
        let (next, _) = self.stage_node(&tx, &st, "register", &json!({ "account": uid, "username": username }), now)?;
        tx.commit()?;
        *st = next;
        Ok(UserInfo { id: uid, username, role: "member".into(), created_at: now })
    }

    // ------------------------------------------------------------------ user services (ADR-0022)
    /// Connects the manager that creates the accounts in the services (default: none).
    pub fn with_provisioner(mut self, p: Arc<dyn Provisioner>) -> Node {
        self.provisioner = p;
        self
    }

    fn offered_in(c: &Connection) -> Result<Vec<String>, NodeError> {
        let raw: Option<String> = c.query_row("SELECT services FROM network WHERE id = 1", [], |r| r.get(0)).optional()?.flatten();
        Ok(match raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()) {
            Some(v) => v,
            None => services::CATALOG.iter().map(|s| s.to_string()).collect(),
        })
    }

    /// The administrator chooses which services this node offers (default: all of them).
    pub fn set_services(&self, offered: &[String]) -> Result<(), NodeError> {
        let all: Vec<String> = services::CATALOG.iter().map(|s| s.to_string()).collect();
        let chosen = services::validate(offered, &all).map_err(NodeError::InvalidInput)?;
        self.lock().execute("UPDATE network SET services = ?1 WHERE id = 1", params![serde_json::to_string(&chosen).unwrap_or_default()])?;
        Ok(())
    }

    fn statuses(&self, c: &Connection, user_id: i64, username: &str) -> Result<Vec<ServiceStatus>, NodeError> {
        let domain: String = c.query_row("SELECT domain FROM network WHERE id = 1", [], |r| r.get(0))?;
        let mut st = c.prepare("SELECT service, status, detail FROM member_services WHERE user_id = ?1")?;
        let rows = st.query_map(params![user_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        let mut out: Vec<ServiceStatus> = rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(service, status, detail)| ServiceStatus { label: services::label(&service).into(), link: if status == "active" || status == "requested" { services::link(&service, username, &domain) } else { None }, service, status, detail })
            .collect();
        out.sort_by_key(|s| services::CATALOG.iter().position(|c| *c == s.service).unwrap_or(99));
        Ok(out)
    }

    /// Creates the accounts in the services, once. The password is used now and not kept.
    fn provision_for(&self, user: &UserInfo, password: &str, wanted: Vec<String>, now: i64) -> Result<Vec<ServiceStatus>, NodeError> {
        let domain: String = self.lock().query_row("SELECT domain FROM network WHERE id = 1", [], |r| r.get(0))?;
        {
            let c = self.lock();
            for s in &wanted {
                c.execute(
                    "INSERT INTO member_services (user_id, service, status, detail, updated_at) VALUES (?1, ?2, 'requested', '', ?3)
                     ON CONFLICT(user_id, service) DO UPDATE SET status = 'requested', detail = '', updated_at = ?3",
                    params![user.id, s, now],
                )?;
            }
        } // no lock is held while the manager works: it can take seconds
        let req = ProvisionRequest { username: user.username.clone(), domain, password: password.to_string(), services: wanted, mail_quota: "1G", blog_quota_mb: 3072 };
        let results = self.provisioner.provision(&req);
        let c = self.lock();
        for (service, outcome) in results {
            let (status, detail) = match outcome {
                Outcome::Done(d) => ("active", d),
                Outcome::Pending(d) => ("requested", d),
                Outcome::Failed(d) => ("failed", d),
            };
            c.execute("UPDATE member_services SET status = ?1, detail = ?2, updated_at = ?3 WHERE user_id = ?4 AND service = ?5", params![status, detail, now, user.id, service])?;
        }
        self.statuses(&c, user.id, &user.username)
    }

    /// Sign-up with a choice of services. The services are checked before the PIN is used.
    pub fn join_with_services(&self, pin: &str, username: &str, password: &str, wanted: &[String], ip: &str, now: i64) -> Result<(UserInfo, Vec<ServiceStatus>), NodeError> {
        let offered = Self::offered_in(&self.lock())?;
        let chosen = services::validate(wanted, &offered).map_err(NodeError::InvalidInput)?;
        let user = self.join(pin, username, password, ip, now)?;
        if chosen.is_empty() {
            return Ok((user, vec![]));
        }
        let st = self.provision_for(&user, password, chosen, now).unwrap_or_default();
        Ok((user, st))
    }

    /// The member's services and what the node offers.
    pub fn my_services(&self, token: &str, now: i64) -> Result<MyServices, NodeError> {
        let c = self.lock();
        let user = Self::session_user_in(&c, token, now).ok_or(NodeError::Unauthorized)?;
        Ok(MyServices { offered: Self::offered_in(&c)?, mine: self.statuses(&c, user.id, &user.username)? })
    }

    /// Asks for more services, or retries those that failed. Needs the password again (it is not kept anywhere).
    pub fn request_services(&self, token: &str, password: &str, wanted: &[String], now: i64) -> Result<Vec<ServiceStatus>, NodeError> {
        let (user, existing) = {
            let c = self.lock();
            let user = Self::session_user_in(&c, token, now).ok_or(NodeError::Unauthorized)?;
            let key = format!("svc:user:{}", user.id);
            if is_locked(&c, &key, 5, now)? {
                return Err(NodeError::Throttled);
            }
            let hash: String = c.query_row("SELECT password_hash FROM users WHERE id = ?1", params![user.id], |r| r.get(0))?;
            if !verify_password(&hash, password) {
                record_failure(&c, &key, 15 * 60, now)?;
                return Err(NodeError::Unauthorized);
            }
            let existing = self.statuses(&c, user.id, &user.username)?;
            (user, existing)
        };
        let offered = Self::offered_in(&self.lock())?;
        let mut merged: Vec<String> = wanted.to_vec();
        merged.extend(existing.iter().filter(|s| s.status != "failed").map(|s| s.service.clone()));
        let chosen = services::validate(&merged, &offered).map_err(NodeError::InvalidInput)?;
        let todo: Vec<String> = chosen.into_iter().filter(|s| wanted.contains(s) && !existing.iter().any(|e| &e.service == s && e.status == "active")).collect();
        if todo.is_empty() {
            return Ok(existing);
        }
        self.provision_for(&user, password, todo, now)
    }

    /// Does this node serve this host name? The proxy asks before it asks for a certificate (on-demand TLS):
    /// only the network's own names and its members' sub-domains get one.
    pub fn tls_allowed(&self, domain: &str) -> bool {
        let d = domain.trim().trim_end_matches('.').to_lowercase();
        let c = self.lock();
        let base: String = match c.query_row("SELECT domain FROM network WHERE id = 1", [], |r| r.get(0)) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let offered = Self::offered_in(&c).unwrap_or_default();
        if d == base {
            return true;
        }
        let Some(label) = d.strip_suffix(&format!(".{base}")) else { return false };
        match label {
            "mail" => return offered.iter().any(|s| s == "mail"),
            "chat" | "matrix" => return offered.iter().any(|s| s == "chat"),
            _ => {}
        }
        if label.contains('.') || label.is_empty() {
            return false;
        }
        c.query_row(
            "SELECT 1 FROM member_services ms JOIN users u ON u.id = ms.user_id
             WHERE u.username = ?1 AND ms.service = 'subdomain' AND ms.status IN ('requested','active')",
            params![label],
            |_| Ok(()),
        )
        .is_ok()
    }

    // ------------------------------------------------------------------ wallet, ledger, parliament (ADR-0020, ADR-0021)
    pub fn network_id(&self) -> &str {
        &self.network_id
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn bank_view(econ: &Economy) -> BankView {
        let b = econ.summary();
        BankView {
            cash: b.cash.0,
            required: b.required.0,
            surplus: b.surplus,
            advance_outstanding: b.advance_outstanding.0,
            mint_balance: b.mint_balance.0,
            emitted_total: b.emitted_total.0,
            members: b.members,
            cycles_closed: b.cycles_closed,
            next_cycle_at: b.next_cycle_at,
            loss_target: b.loss_target.0,
            loss_held: b.loss_held.0,
        }
    }

    /// Applies one entry to a copy of the state and writes it to the ledger (inside the caller's transaction).
    /// The caller commits and then replaces the state with the returned copy.
    fn stage(&self, c: &Connection, st: &State, kind: &str, payload: &str, signer: &str, signature: &str, t: i64) -> Result<(State, Applied), NodeError> {
        let e = ledger::Entry { seq: st.len + 1, time: t, kind: kind.into(), payload: payload.into(), signer: signer.into(), signature: signature.into() };
        let mut next = st.clone();
        let applied = next.apply(&e, &self.node_key.verifying_key(), &self.network_id).map_err(apply_to_node)?;
        let (seq, _) = ledger::append(c, kind, t, payload, signer, signature)?;
        if seq != e.seq {
            return Err(NodeError::Db("ledger sequence out of step".into()));
        }
        Ok((next, applied))
    }

    fn stage_node(&self, c: &Connection, st: &State, kind: &str, payload: &Value, now: i64) -> Result<(State, Applied), NodeError> {
        let t = now.max(st.last_time);
        let p = payload.to_string();
        let sig = hex(&self.node_key.sign(&ledger::node_message(kind, t, &p)).to_bytes());
        self.stage(c, st, kind, &p, "node", &sig, t)
    }

    /// Registers the member's public key. The browser derived it from the recovery phrase; the node never sees the phrase.
    /// Needs the session, the password again, and a signature proving the member holds the private key.
    pub fn register_key(&self, token: &str, password: &str, public_key_hex: &str, signature_hex: &str, now: i64) -> Result<(), NodeError> {
        let mut c = self.lock();
        let user = Self::session_user_in(&c, token, now).ok_or(NodeError::Unauthorized)?;
        let limit_key = format!("key:user:{}", user.id);
        if is_locked(&c, &limit_key, 5, now)? {
            return Err(NodeError::Throttled);
        }
        let hash: String = c.query_row("SELECT password_hash FROM users WHERE id = ?1", params![user.id], |r| r.get(0))?;
        if !verify_password(&hash, password) {
            record_failure(&c, &limit_key, 15 * 60, now)?;
            return Err(NodeError::Unauthorized);
        }
        let pk = public_key_hex.trim().to_lowercase();
        if ledger::vk_from_hex(&pk).is_none() {
            return Err(NodeError::InvalidInput("Chiave pubblica non valida.".into()));
        }
        let mut st = self.lock_state();
        let payload = json!({ "account": user.id, "username": user.username, "public_key": pk, "proof": signature_hex.trim() });
        let tx = c.transaction()?;
        let (next, _) = self.stage_node(&tx, &st, "key", &payload, now)?;
        tx.commit()?;
        *st = next;
        Ok(())
    }

    /// Something a member signed in the browser (a payment, a proposal, a vote). `payload` is the exact JSON string signed.
    fn submit_signed(&self, token: &str, expected: &str, payload: &str, signature_hex: &str, now: i64) -> Result<(i64, Applied), NodeError> {
        let mut c = self.lock();
        let user = Self::session_user_in(&c, token, now).ok_or(NodeError::Unauthorized)?;
        if payload.len() > 8192 {
            return Err(NodeError::InvalidInput("Movimento troppo grande.".into()));
        }
        let v: Value = serde_json::from_str(payload).map_err(|_| NodeError::InvalidInput("Movimento non valido.".into()))?;
        if v.get("kind").and_then(Value::as_str) != Some(expected) {
            return Err(NodeError::InvalidInput("Movimento non valido.".into()));
        }
        let mut st = self.lock_state();
        let t = now.max(st.last_time);
        let tx = c.transaction()?;
        let (next, applied) = self.stage(&tx, &st, expected, payload, &format!("user:{}", user.id), signature_hex.trim(), t)?;
        tx.commit()?;
        let seq = next.len;
        *st = next;
        Ok((seq, applied))
    }

    /// A payment signed by the member in the browser.
    pub fn submit_pay(&self, token: &str, payload: &str, signature_hex: &str, now: i64) -> Result<PayOutcome, NodeError> {
        match self.submit_signed(token, "pay", payload, signature_hex, now)? {
            (seq, Applied::Pay(r)) => Ok(PayOutcome { seq, cost: r.cost.0, seller_receives: r.seller_receives.0, fee_buyer: r.fee_buyer.0, fee_seller: r.fee_seller.0, borrowed: r.borrowed.0 }),
            _ => Err(NodeError::InvalidInput("Movimento non valido.".into())),
        }
    }

    /// A proposal signed by the member. Returns its number.
    pub fn submit_proposal(&self, token: &str, payload: &str, signature_hex: &str, now: i64) -> Result<i64, NodeError> {
        match self.submit_signed(token, "proposal", payload, signature_hex, now)? {
            (_, Applied::Proposal(id)) => Ok(id),
            _ => Err(NodeError::InvalidInput("Proposta non valida.".into())),
        }
    }

    /// A vote signed by the member (the last vote before the deadline counts).
    pub fn submit_vote(&self, token: &str, payload: &str, signature_hex: &str, now: i64) -> Result<(), NodeError> {
        self.submit_signed(token, "vote", payload, signature_hex, now).map(|_| ())
    }

    /// The proposals, newest first, with the open ballot.
    pub fn proposals(&self, token: &str, now: i64) -> Result<Vec<ProposalView>, NodeError> {
        let c = self.lock();
        let user = Self::session_user_in(&c, token, now).ok_or(NodeError::Unauthorized)?;
        let st = self.lock_state();
        let me = user.id as u64;
        let name = |id: u64| st.names.get(&id).cloned().unwrap_or_default();
        let mut out = Vec::new();
        for p in st.proposals.values().rev().take(100) {
            let (status, t) = match (&p.status, &p.result) {
                (Status::Closed(o), Some(t)) => (o.clone(), t.clone()),
                _ => ("open".to_string(), st.tally(p)),
            };
            let eligible_now = st.key_seq.get(&me).map(|s| *s < p.id).unwrap_or(false);
            out.push(ProposalView {
                id: p.id,
                kind: p.kind.clone(),
                title: p.title.clone(),
                body: p.body.clone(),
                proposer: name(p.proposer),
                amount: p.amount,
                to: p.to.map(name),
                created: p.created,
                deadline: p.deadline,
                status: status.clone(),
                needs: if p.kind == "mint" { "two_thirds".into() } else { "simple".into() },
                yes: t.yes,
                no: t.no,
                abstain: t.abstain,
                eligible: t.eligible,
                quorum_needed: (st.parl.quorum_bps as u64 * t.eligible + 9_999) / 10_000,
                votes: p.votes.iter().map(|(u, ch)| VoteLine { user: name(*u), choice: ch.clone() }).collect(),
                my_vote: p.votes.get(&me).cloned(),
                can_vote: status == "open" && now < p.deadline && eligible_now,
            });
        }
        Ok(out)
    }

    /// Closes the proposals whose time is up (the node does it by itself). Returns how many.
    /// The entry states the result, but the ledger only accepts it if it matches the votes.
    pub fn close_due_proposals(&self, now: i64) -> Result<usize, NodeError> {
        let mut c = self.lock();
        let mut st = self.lock_state();
        let mut closed = 0;
        loop {
            let due = st.proposals.values().find(|p| p.status == Status::Open && p.deadline <= now).cloned();
            let Some(p) = due else { break };
            let t = st.tally(&p);
            let payload = json!({ "proposal": p.id, "yes": t.yes, "no": t.no, "abstain": t.abstain, "eligible": t.eligible, "outcome": st.outcome(&p, &t) });
            let tx = c.transaction()?;
            let (next, _) = self.stage_node(&tx, &st, "proposal_close", &payload, now)?;
            tx.commit()?;
            *st = next;
            closed += 1;
        }
        Ok(closed)
    }

    /// The member's wallet and the bank's figures (what the wallet page shows).
    pub fn wallet(&self, token: &str, now: i64) -> Result<WalletView, NodeError> {
        let c = self.lock();
        let user = Self::session_user_in(&c, token, now).ok_or(NodeError::Unauthorized)?;
        let currency: String = c.query_row("SELECT currency_symbol FROM network WHERE id = 1", [], |r| r.get(0))?;
        let st = self.lock_state();
        let uid = user.id as u64;
        let acc = st.economy.account(uid).ok_or_else(|| NodeError::Db("account not in the economy".into()))?;
        let mut recent = Vec::new();
        let mut q = c.prepare("SELECT seq, time, payload FROM ledger WHERE kind = 'pay' ORDER BY seq DESC LIMIT 400")?;
        let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            let (seq, time, payload) = row?;
            if let Ok(v) = serde_json::from_str::<Value>(&payload) {
                let (from, to) = (v["from"].as_str().unwrap_or(""), v["to"].as_str().unwrap_or(""));
                if from == user.username || to == user.username {
                    recent.push(LedgerLine { seq, time, from: from.into(), to: to.into(), amount: v["amount"].as_u64().unwrap_or(0), memo: v["memo"].as_str().unwrap_or("").into() });
                    if recent.len() == 20 {
                        break;
                    }
                }
            }
        }
        Ok(WalletView {
            username: user.username,
            network_id: self.network_id.clone(),
            currency_symbol: currency,
            public_key: st.keys.get(&uid).cloned(),
            next_nonce: st.nonces.get(&uid).copied().unwrap_or(0) + 1,
            balance: acc.balance.0,
            debt: st.economy.debt_total(uid).0,
            fido: acc.fido.0,
            credit_available: st.economy.credit_available(uid).0,
            blocked: st.economy.is_blocked(uid, now.max(0) as u64),
            started: acc.first_tx.is_some(),
            bank: Self::bank_view(&st.economy),
            recent,
        })
    }

    /// Closes the network cycles that are due (the bank does it by itself). The cycle pays exactly what the parliament
    /// approved: the ledger refuses a cycle that pays anything else. Returns how many cycles were closed.
    pub fn close_due_cycles(&self, now: i64) -> Result<usize, NodeError> {
        let mut c = self.lock();
        let mut st = self.lock_state();
        let mut closed = 0;
        while st.economy.cycle_due(now.max(0) as u64) {
            let mut probe = (*st).clone();
            probe.prune_queues();
            let input = probe.cycle_input();
            let tx = c.transaction()?;
            let (next, _) = self.stage_node(&tx, &st, "close_cycle", &json!({ "input": input }), now)?;
            tx.commit()?;
            *st = next;
            closed += 1;
        }
        Ok(closed)
    }

    /// Re-reads the whole ledger from the database and verifies it. Returns the number of entries.
    pub fn verify_ledger(&self) -> Result<i64, NodeError> {
        let c = self.lock();
        let st = ledger::replay(&ledger::all(&c)?, &self.node_key.verifying_key(), &self.network_id)?;
        Ok(st.map(|s| s.len).unwrap_or(0))
    }
}
