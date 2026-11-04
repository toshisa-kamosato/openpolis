// SPDX-License-Identifier: GPL-2.0-or-later
//! The signed ledger (level L0, ADR-0020) and the parliament (ADR-0021): a hash chain in SQLite. Every fact of the
//! network is one entry. `State::apply` is the single place where an entry changes the state, both when the node
//! accepts a new entry and when it replays the whole ledger at start-up: the rules cannot diverge.
use std::collections::{BTreeMap, HashMap};

use ed25519_dalek::{Signature, VerifyingKey};
use lux_economy::{Amount, CycleInput, CycleReport, EconError, Economy, Params, PayReceipt, Payout};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::auth::hex;
use crate::error::NodeError;

pub const ZERO_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const DAY: i64 = 86_400;

// ---------------------------------------------------------------- the chain
#[derive(Clone, Debug)]
pub struct Row {
    pub seq: i64,
    pub prev_hash: String,
    pub time: i64,
    pub kind: String,
    pub payload: String,
    pub signer: String,
    pub signature: String,
    pub hash: String,
}

pub fn entry_hash(seq: i64, prev: &str, time: i64, kind: &str, payload: &str, signer: &str, sig: &str) -> String {
    let (s_seq, s_time) = (seq.to_string(), time.to_string());
    let mut h = Sha256::new();
    for part in [s_seq.as_str(), prev, s_time.as_str(), kind, payload, signer, sig] {
        h.update((part.len() as u64).to_le_bytes()); // length-prefixed: no two different entries share a hash input
        h.update(part.as_bytes());
    }
    hex(&h.finalize())
}

/// What the node signs for its own entries.
pub fn node_message(kind: &str, time: i64, payload: &str) -> Vec<u8> {
    format!("openpolis-ledger-v1\n{kind}\n{time}\n{payload}").into_bytes()
}

/// What a member signs to prove they hold the key they register.
pub fn registration_message(network_id: &str, username: &str, public_key_hex: &str) -> String {
    format!("register-key:v1:{network_id}:{username}:{public_key_hex}")
}

pub fn vk_from_hex(h: &str) -> Option<VerifyingKey> {
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut b = [0u8; 32];
    for i in 0..32 {
        b[i] = u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok()?;
    }
    VerifyingKey::from_bytes(&b).ok()
}

pub fn verify(vk: &VerifyingKey, msg: &[u8], sig_hex: &str) -> bool {
    if sig_hex.len() != 128 || !sig_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return false;
    }
    let mut b = [0u8; 64];
    for i in 0..64 {
        match u8::from_str_radix(&sig_hex[i * 2..i * 2 + 2], 16) {
            Ok(x) => b[i] = x,
            Err(_) => return false,
        }
    }
    vk.verify_strict(msg, &Signature::from_bytes(&b)).is_ok()
}

pub fn last(c: &Connection) -> Result<Option<(i64, String, i64)>, NodeError> {
    Ok(c.query_row("SELECT seq, hash, time FROM ledger ORDER BY seq DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?)
}

pub fn append(c: &Connection, kind: &str, time: i64, payload: &str, signer: &str, signature: &str) -> Result<(i64, String), NodeError> {
    let (seq, prev) = match last(c)? {
        Some((s, h, _)) => (s + 1, h),
        None => (1, ZERO_HASH.to_string()),
    };
    let hash = entry_hash(seq, &prev, time, kind, payload, signer, signature);
    c.execute(
        "INSERT INTO ledger (seq, prev_hash, time, kind, payload, signer, signature, hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![seq, prev, time, kind, payload, signer, signature, hash],
    )?;
    Ok((seq, hash))
}

pub fn all(c: &Connection) -> Result<Vec<Row>, NodeError> {
    let mut st = c.prepare("SELECT seq, prev_hash, time, kind, payload, signer, signature, hash FROM ledger ORDER BY seq")?;
    let rows = st.query_map([], |r| {
        Ok(Row { seq: r.get(0)?, prev_hash: r.get(1)?, time: r.get(2)?, kind: r.get(3)?, payload: r.get(4)?, signer: r.get(5)?, signature: r.get(6)?, hash: r.get(7)? })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

// ---------------------------------------------------------------- the parliament's rules
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParliamentParams {
    /// Share of the eligible members that must vote (yes, no or abstain), in basis points.
    pub quorum_bps: u32,
    /// Share of yes among yes+no that must be exceeded, in basis points (5000 = more yes than no).
    pub majority_bps: u32,
    pub max_open_per_member: u32,
    pub min_days: u32,
    pub max_days: u32,
    /// An emission of money needs a qualified majority: yes / (yes + no) >= num / den (default two thirds).
    #[serde(default = "default_qualified_num")]
    pub qualified_num: u32,
    #[serde(default = "default_qualified_den")]
    pub qualified_den: u32,
    /// A member can be declared abandoned only if they signed nothing for this many days (three cycles).
    #[serde(default = "default_inactive_days")]
    pub abandon_inactive_days: u32,
}

fn default_qualified_num() -> u32 {
    2
}
fn default_qualified_den() -> u32 {
    3
}
fn default_inactive_days() -> u32 {
    84
}

impl Default for ParliamentParams {
    fn default() -> Self {
        ParliamentParams {
            quorum_bps: 2000,
            majority_bps: 5000,
            max_open_per_member: 3,
            min_days: 1,
            max_days: 28,
            qualified_num: default_qualified_num(),
            qualified_den: default_qualified_den(),
            abandon_inactive_days: default_inactive_days(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Tally {
    pub yes: u64,
    pub no: u64,
    pub abstain: u64,
    pub eligible: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Open,
    /// "accepted", "rejected", "no_quorum" or "void" (accepted, but the member came back to life)
    Closed(String),
}

#[derive(Clone, Debug)]
pub struct Proposal {
    pub id: i64,
    pub proposer: u64,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub amount: u64,
    pub to: Option<u64>,
    pub created: i64,
    pub deadline: i64,
    pub votes: BTreeMap<u64, String>,
    pub status: Status,
    pub result: Option<Tally>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueItem {
    pub proposal: i64,
    pub to: u64,
    pub remaining: u64,
}

// ---------------------------------------------------------------- applying an entry
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrKind {
    Invalid,
    Conflict,
}

#[derive(Clone, Debug)]
pub struct ApplyError {
    pub kind: ErrKind,
    pub msg: String,
}

fn inv<T>(m: impl Into<String>) -> Result<T, ApplyError> {
    Err(ApplyError { kind: ErrKind::Invalid, msg: m.into() })
}
fn conf<T>(m: impl Into<String>) -> Result<T, ApplyError> {
    Err(ApplyError { kind: ErrKind::Conflict, msg: m.into() })
}

fn econ_err(e: EconError) -> ApplyError {
    let (kind, msg) = match e {
        EconError::InsufficientFunds { needed, available } => (ErrKind::Invalid, format!("fondi insufficienti: servono {needed} LUX, disponibili {available} (saldo più fido)")),
        EconError::Blocked(_) => (ErrKind::Conflict, "il tuo conto è bloccato da un debito scaduto: puoi ricevere LUX, non spenderli".into()),
        EconError::BankIlliquid { .. } => (ErrKind::Conflict, "la banca non può finanziare ora questo pagamento".into()),
        EconError::SameAccount => (ErrKind::Invalid, "non puoi pagare te stesso".into()),
        EconError::ZeroAmount => (ErrKind::Invalid, "l'importo deve essere maggiore di zero".into()),
        EconError::UnknownAccount(_) => (ErrKind::Invalid, "destinatario non valido".into()),
        EconError::AccountClosed(_) => (ErrKind::Invalid, "l'account è chiuso o segnato come abbandonato".into()),
        EconError::MintExhausted => (ErrKind::Conflict, "la rete non ha più moneta per nuovi iscritti: serve una nuova emissione votata".into()),
        other => (ErrKind::Conflict, other.to_string()),
    };
    ApplyError { kind, msg }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub seq: i64,
    pub time: i64,
    pub kind: String,
    pub payload: String,
    pub signer: String,
    pub signature: String,
}

/// What an applied entry produced (for the caller that needs to answer a request).
#[derive(Clone, Debug)]
#[allow(dead_code)] // `Closed` and `Cycle` are for callers that report results; the node does not need them yet
pub enum Applied {
    Nothing,
    Pay(PayReceipt),
    Proposal(i64),
    Closed(String),
    Cycle(Box<CycleReport>),
}

/// The state obtained by replaying the ledger.
#[derive(Clone)]
pub struct State {
    pub economy: Economy,
    pub ids: HashMap<String, u64>,
    pub names: HashMap<u64, String>,
    pub keys: HashMap<u64, String>,
    /// Ledger position of each member's key: only members whose key came before a proposal may vote on it.
    pub key_seq: HashMap<u64, i64>,
    /// One counter per member for everything they sign (payments, proposals, votes).
    pub nonces: HashMap<u64, i64>,
    /// When each member last signed something (a payment, a proposal, a vote) or joined.
    pub last_action: HashMap<u64, i64>,
    pub parl: ParliamentParams,
    pub proposals: BTreeMap<i64, Proposal>,
    pub works: Vec<QueueItem>,
    pub expenses: Vec<QueueItem>,
    pub last_time: i64,
    pub len: i64,
}

fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, ApplyError> {
    match v.get(k).and_then(Value::as_str) {
        Some(s) => Ok(s),
        None => inv(format!("campo «{k}» mancante")),
    }
}
fn num(v: &Value, k: &str) -> Result<u64, ApplyError> {
    match v.get(k).and_then(Value::as_u64) {
        Some(n) => Ok(n),
        None => inv(format!("campo «{k}» mancante")),
    }
}

impl State {
    fn genesis(e: &Entry, node_vk: &VerifyingKey) -> Result<State, ApplyError> {
        if e.signer != "node" || !verify(node_vk, &node_message(&e.kind, e.time, &e.payload), &e.signature) {
            return inv("firma del nodo non valida");
        }
        let v: Value = serde_json::from_str(&e.payload).or_else(|_| inv("contenuto non valido"))?;
        let params: Params = serde_json::from_value(v.get("params").cloned().unwrap_or(Value::Null)).or_else(|_| inv("parametri non validi"))?;
        let parl: ParliamentParams = match v.get("parliament") {
            Some(p) => serde_json::from_value(p.clone()).or_else(|_| inv("parametri del parlamento non validi"))?,
            None => ParliamentParams::default(), // a ledger created before the parliament existed
        };
        Ok(State {
            economy: Economy::new(params, e.time as u64),
            ids: HashMap::new(),
            names: HashMap::new(),
            keys: HashMap::new(),
            key_seq: HashMap::new(),
            nonces: HashMap::new(),
            last_action: HashMap::new(),
            parl,
            proposals: BTreeMap::new(),
            works: Vec::new(),
            expenses: Vec::new(),
            last_time: e.time,
            len: e.seq,
        })
    }

    fn account_open(&self, id: u64) -> bool {
        matches!(self.economy.account(id), Some(a) if !a.closed && !a.abandoned)
    }

    /// Members entitled to vote on a proposal: they had their key before the proposal existed and are still members.
    pub fn eligible(&self, p: &Proposal) -> Vec<u64> {
        let mut v: Vec<u64> = self.key_seq.iter().filter(|(id, s)| **s < p.id && self.account_open(**id)).map(|(id, _)| *id).collect();
        v.sort_unstable();
        v
    }

    pub fn tally(&self, p: &Proposal) -> Tally {
        let el = self.eligible(p);
        let (mut yes, mut no, mut abstain) = (0, 0, 0);
        for (voter, choice) in &p.votes {
            if el.binary_search(voter).is_ok() {
                match choice.as_str() {
                    "yes" => yes += 1,
                    "no" => no += 1,
                    _ => abstain += 1,
                }
            }
        }
        Tally { yes, no, abstain, eligible: el.len() as u64 }
    }

    /// The result of a vote. An emission of money needs a qualified majority; an abandonment is void if the member has
    /// signed something since the proposal was made (it exists to close dead accounts, not to confiscate live ones).
    pub fn outcome(&self, p: &Proposal, t: &Tally) -> &'static str {
        let voted = t.yes + t.no + t.abstain;
        if t.eligible == 0 || voted * 10_000 < self.parl.quorum_bps as u64 * t.eligible {
            return "no_quorum";
        }
        let accepted = if p.kind == "mint" {
            t.yes > 0 && t.yes * self.parl.qualified_den as u64 >= self.parl.qualified_num as u64 * (t.yes + t.no)
        } else {
            t.yes * 10_000 > self.parl.majority_bps as u64 * (t.yes + t.no)
        };
        if !accepted {
            return "rejected";
        }
        if p.kind == "abandon" {
            let target = p.to.unwrap_or(u64::MAX);
            if !self.account_open(target) || self.last_action.get(&target).copied().unwrap_or(0) >= p.created {
                // at the proposal's creation the member had been quiet for days: any later signature is a sign of life
                return "void";
            }
        }
        "accepted"
    }

    /// Forgets payouts whose recipient is no longer a member (the engine would refuse the whole cycle).
    pub fn prune_queues(&mut self) {
        let ok: Vec<u64> = self.names.keys().copied().filter(|id| self.account_open(*id)).collect();
        self.works.retain(|i| ok.contains(&i.to));
        self.expenses.retain(|i| ok.contains(&i.to));
    }

    /// What the next cycle must pay: all approved expenses, then the approved works in order.
    pub fn cycle_input(&self) -> CycleInput {
        let item = |i: &QueueItem| Payout { to: i.to, amount: Amount(i.remaining), memo: format!("proposta #{}", i.proposal) };
        CycleInput { network_expenses: self.expenses.iter().map(item).collect(), works: self.works.iter().map(item).collect() }
    }

    fn after_cycle(&mut self, r: &CycleReport) {
        self.expenses.clear(); // expenses are always paid in full
        let mut avail = r.works_paid.0; // the engine pays the works in order, each up to what is left
        for w in &mut self.works {
            let paid = w.remaining.min(avail);
            w.remaining -= paid;
            avail -= paid;
        }
        self.works.retain(|w| w.remaining > 0);
    }

    /// The single place where an entry changes the state. Used live and when replaying the ledger.
    pub fn apply(&mut self, e: &Entry, node_vk: &VerifyingKey, network_id: &str) -> Result<Applied, ApplyError> {
        if e.seq != self.len + 1 {
            return inv("numerazione interrotta");
        }
        if e.time < self.last_time {
            return inv("tempo che torna indietro");
        }
        let v: Value = serde_json::from_str(&e.payload).or_else(|_| inv("contenuto non valido"))?;
        let mut applied = Applied::Nothing;
        match e.kind.as_str() {
            "pay" | "proposal" | "vote" => applied = self.apply_user(e, &v, network_id)?,
            node_kind => {
                if e.signer != "node" {
                    return inv(format!("il firmatario non è ammesso per «{node_kind}»"));
                }
                if !verify(node_vk, &node_message(&e.kind, e.time, &e.payload), &e.signature) {
                    return inv("firma del nodo non valida");
                }
                match node_kind {
                    "register" => {
                        let (id, name) = (num(&v, "account")?, text(&v, "username")?.to_string());
                        self.economy.register(id, e.time as u64).map_err(econ_err)?;
                        self.ids.insert(name.clone(), id);
                        self.names.insert(id, name);
                        self.last_action.insert(id, e.time);
                    }
                    "key" => {
                        let (id, pk, proof) = (num(&v, "account")?, text(&v, "public_key")?.to_string(), text(&v, "proof")?);
                        let Some(name) = self.names.get(&id) else { return inv("account sconosciuto") };
                        if self.keys.contains_key(&id) {
                            return conf("hai già un portafoglio: usa la tua frase di recupero per ripristinarlo");
                        }
                        let Some(vk) = vk_from_hex(&pk) else { return inv("chiave non valida") };
                        if !verify(&vk, registration_message(network_id, name, &pk).as_bytes(), proof) {
                            return inv("prova di possesso della chiave non valida");
                        }
                        self.keys.insert(id, pk);
                        self.key_seq.insert(id, e.seq);
                        self.last_action.insert(id, e.time);
                    }
                    "proposal_close" => applied = self.apply_close(e, &v)?,
                    "close_cycle" => {
                        self.prune_queues();
                        let input: CycleInput = serde_json::from_value(v.get("input").cloned().unwrap_or(Value::Null)).or_else(|_| inv("ingresso del ciclo non valido"))?;
                        if input != self.cycle_input() {
                            return inv("l'ingresso del ciclo non corrisponde alle proposte approvate");
                        }
                        let report = self.economy.close_cycle(e.time as u64, &input).map_err(econ_err)?;
                        self.after_cycle(&report);
                        applied = Applied::Cycle(Box::new(report));
                    }
                    _ => return inv("tipo di voce sconosciuto"),
                }
            }
        }
        self.len = e.seq;
        self.last_time = e.time;
        if let Err(m) = self.economy.check_invariants() {
            return inv(m);
        }
        Ok(applied)
    }

    fn apply_user(&mut self, e: &Entry, v: &Value, network_id: &str) -> Result<Applied, ApplyError> {
        let from = text(v, "from")?;
        let Some(&uid) = self.ids.get(from) else { return inv("mittente sconosciuto") };
        if e.signer != format!("user:{uid}") {
            return inv("il firmatario non è il mittente");
        }
        let Some(key) = self.keys.get(&uid).and_then(|k| vk_from_hex(k)) else { return conf("crea prima il tuo portafoglio") };
        if !verify(&key, e.payload.as_bytes(), &e.signature) {
            return inv("firma dell'utente non valida");
        }
        if v.get("v").and_then(Value::as_i64) != Some(1) || v.get("kind").and_then(Value::as_str) != Some(e.kind.as_str()) || text(v, "net")? != network_id {
            return inv("movimento non valido");
        }
        let nonce = v.get("nonce").and_then(Value::as_i64).unwrap_or(-1);
        if nonce != self.nonces.get(&uid).copied().unwrap_or(0) + 1 {
            return conf("movimento già inviato o fuori sequenza: aggiorna la pagina");
        }
        let applied = match e.kind.as_str() {
            "pay" => {
                if v.get("memo").and_then(Value::as_str).map(|m| m.chars().count() <= 80) != Some(true) {
                    return inv("nota troppo lunga (massimo 80 caratteri)");
                }
                let Some(&tid) = self.ids.get(text(v, "to")?) else { return inv("destinatario inesistente") };
                let r = self.economy.pay(uid, tid, Amount(num(v, "amount")?), e.time as u64).map_err(econ_err)?;
                Applied::Pay(r)
            }
            "proposal" => self.apply_proposal(e, v, uid)?,
            _ => self.apply_vote(e, v, uid)?,
        };
        self.nonces.insert(uid, nonce);
        self.last_action.insert(uid, e.time);
        Ok(applied)
    }

    fn apply_proposal(&mut self, e: &Entry, v: &Value, uid: u64) -> Result<Applied, ApplyError> {
        let kind = text(v, "type")?.to_string();
        if !["text", "mint", "works", "expense", "abandon"].contains(&kind.as_str()) {
            return inv("tipo di proposta sconosciuto");
        }
        let title = text(v, "title")?.trim().to_string();
        let body = text(v, "body")?.trim().to_string();
        if title.is_empty() || title.chars().count() > 120 {
            return inv("il titolo deve avere da 1 a 120 caratteri");
        }
        if body.chars().count() > 2000 {
            return inv("il testo è troppo lungo (massimo 2000 caratteri)");
        }
        let days = num(v, "days")?;
        if days < self.parl.min_days as u64 || days > self.parl.max_days as u64 {
            return inv(format!("la votazione deve durare da {} a {} giorni", self.parl.min_days, self.parl.max_days));
        }
        let amount = v.get("amount").and_then(Value::as_u64).unwrap_or(0);
        let to_name = v.get("to").and_then(Value::as_str).unwrap_or("");
        let mut to = None;
        match kind.as_str() {
            "text" => {
                if amount != 0 || !to_name.is_empty() {
                    return inv("una proposta di testo non ha importo né destinatario");
                }
            }
            "abandon" => {
                if amount != 0 {
                    return inv("una dichiarazione di abbandono non ha importo");
                }
                let target = match self.ids.get(to_name) {
                    Some(&id) if self.account_open(id) => id,
                    _ => return inv("il destinatario non è un membro"),
                };
                if target == uid {
                    return inv("non puoi proporre l'abbandono di te stesso");
                }
                let idle = e.time - self.last_action.get(&target).copied().unwrap_or(0);
                if idle < self.parl.abandon_inactive_days as i64 * DAY {
                    return inv(format!("il membro è stato attivo negli ultimi {} giorni", self.parl.abandon_inactive_days));
                }
                if self.proposals.values().any(|p| p.kind == "abandon" && p.to == Some(target) && p.status == Status::Open) {
                    return conf("esiste già una proposta aperta per questo membro");
                }
                to = Some(target);
            }
            _ => {
                if amount == 0 || amount > self.economy.emitted_total().0 {
                    return inv("importo non valido (da 0.01 LUX fino alla moneta già emessa)");
                }
                if kind != "mint" {
                    match self.ids.get(to_name) {
                        Some(&id) if self.account_open(id) => to = Some(id),
                        _ => return inv("il destinatario non è un membro"),
                    }
                } else if !to_name.is_empty() {
                    return inv("un'emissione non ha destinatario");
                }
            }
        }
        let open = self.proposals.values().filter(|p| p.proposer == uid && p.status == Status::Open).count();
        if open >= self.parl.max_open_per_member as usize {
            return conf(format!("hai già {} proposte aperte: aspetta che si chiuda una", self.parl.max_open_per_member));
        }
        let id = e.seq;
        self.proposals.insert(
            id,
            Proposal { id, proposer: uid, kind, title, body, amount, to, created: e.time, deadline: e.time + days as i64 * DAY, votes: BTreeMap::new(), status: Status::Open, result: None },
        );
        Ok(Applied::Proposal(id))
    }

    fn apply_vote(&mut self, e: &Entry, v: &Value, uid: u64) -> Result<Applied, ApplyError> {
        let pid = v.get("proposal").and_then(Value::as_i64).unwrap_or(-1);
        let choice = text(v, "choice")?.to_string();
        if !["yes", "no", "abstain"].contains(&choice.as_str()) {
            return inv("scelta non valida");
        }
        let eligible = self.key_seq.get(&uid).copied();
        let Some(p) = self.proposals.get_mut(&pid) else { return inv("proposta inesistente") };
        if p.status != Status::Open || e.time >= p.deadline {
            return conf("la votazione è chiusa");
        }
        if !matches!(eligible, Some(s) if s < p.id) {
            return conf("non hai diritto di voto su questa proposta: hai creato il portafoglio dopo la sua apertura");
        }
        p.votes.insert(uid, choice);
        Ok(Applied::Nothing)
    }

    fn apply_close(&mut self, e: &Entry, v: &Value) -> Result<Applied, ApplyError> {
        let pid = v.get("proposal").and_then(Value::as_i64).unwrap_or(-1);
        let Some(p) = self.proposals.get(&pid).cloned() else { return inv("proposta inesistente") };
        if p.status != Status::Open {
            return inv("proposta già chiusa");
        }
        if e.time < p.deadline {
            return inv("chiusura prima della scadenza");
        }
        let t = self.tally(&p);
        let outcome = self.outcome(&p, &t);
        let claimed = (num(v, "yes")?, num(v, "no")?, num(v, "abstain")?, num(v, "eligible")?, text(v, "outcome")?);
        if claimed != (t.yes, t.no, t.abstain, t.eligible, outcome) {
            return inv("l'esito dichiarato non corrisponde ai voti");
        }
        if outcome == "accepted" {
            match p.kind.as_str() {
                "mint" => self.economy.mint_emit_by_vote(Amount(p.amount), p.id as u64, e.time as u64).map_err(econ_err)?,
                "abandon" => self.economy.mark_abandoned(p.to.unwrap_or(0), e.time as u64).map_err(econ_err)?,
                "works" => self.works.push(QueueItem { proposal: p.id, to: p.to.unwrap_or(0), remaining: p.amount }),
                "expense" => self.expenses.push(QueueItem { proposal: p.id, to: p.to.unwrap_or(0), remaining: p.amount }),
                _ => {}
            }
        }
        if let Some(pm) = self.proposals.get_mut(&pid) {
            pm.status = Status::Closed(outcome.to_string());
            pm.result = Some(t);
        }
        Ok(Applied::Closed(outcome.to_string()))
    }
}

/// Replays every entry: chain, hashes, signatures and rules. Returns `None` for an empty ledger.
pub fn replay(rows: &[Row], node_vk: &VerifyingKey, network_id: &str) -> Result<Option<State>, NodeError> {
    let mut st: Option<State> = None;
    let mut prev = ZERO_HASH.to_string();
    let mut last_time = 0i64;
    for (i, r) in rows.iter().enumerate() {
        let bad = |m: &str| NodeError::LedgerCorrupt(format!("voce {}: {m}", r.seq));
        if r.seq != i as i64 + 1 {
            return Err(bad("numerazione interrotta"));
        }
        if r.prev_hash != prev {
            return Err(bad("catena interrotta"));
        }
        if r.hash != entry_hash(r.seq, &r.prev_hash, r.time, &r.kind, &r.payload, &r.signer, &r.signature) {
            return Err(bad("hash non corrispondente"));
        }
        if r.time < last_time {
            return Err(bad("tempo che torna indietro"));
        }
        let e = Entry { seq: r.seq, time: r.time, kind: r.kind.clone(), payload: r.payload.clone(), signer: r.signer.clone(), signature: r.signature.clone() };
        if r.kind == "genesis" {
            if st.is_some() {
                return Err(bad("genesi doppia"));
            }
            st = Some(State::genesis(&e, node_vk).map_err(|x| bad(&x.msg))?);
        } else {
            let s = st.as_mut().ok_or_else(|| bad("manca la genesi"))?;
            s.apply(&e, node_vk, network_id).map_err(|x| bad(&x.msg))?;
        }
        prev = r.hash.clone();
        last_time = r.time;
    }
    Ok(st)
}
