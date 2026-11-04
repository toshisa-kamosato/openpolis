// SPDX-License-Identifier: GPL-2.0-or-later
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeError {
    InvalidInput(String),
    /// Wrong, expired or already used PIN: one single message on purpose (no oracle for the attacker).
    InvalidPin,
    Unauthorized,
    Forbidden,
    Throttled,
    Conflict(String),
    AlreadyInitialized,
    NotInitialized,
    Db(String),
    /// The ledger does not verify (broken chain, bad signature, a rule that no longer holds): the node refuses to start.
    LedgerCorrupt(String),
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeError::InvalidInput(m) => write!(f, "{m}"),
            NodeError::InvalidPin => write!(f, "PIN non valido o scaduto"),
            NodeError::Unauthorized => write!(f, "Accesso richiesto"),
            NodeError::Forbidden => write!(f, "Non hai il permesso"),
            NodeError::Throttled => write!(f, "Troppi tentativi: riprova più tardi"),
            NodeError::Conflict(m) => write!(f, "{m}"),
            NodeError::AlreadyInitialized => write!(f, "Questo nodo ha già una rete"),
            NodeError::NotInitialized => write!(f, "Questo nodo non ha ancora una rete: esegui `luxd init`"),
            NodeError::Db(m) => write!(f, "errore del database: {m}"),
            NodeError::LedgerCorrupt(m) => write!(f, "registro non valido: {m}"),
        }
    }
}

impl std::error::Error for NodeError {}

impl From<rusqlite::Error> for NodeError {
    fn from(e: rusqlite::Error) -> Self {
        NodeError::Db(e.to_string())
    }
}
