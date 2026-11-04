// SPDX-License-Identifier: GPL-2.0-or-later
use crate::engine::{AccountId, Time};
use crate::money::Amount;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconError {
    UnknownAccount(AccountId),
    AccountExists(AccountId),
    /// The account is closed or marked as abandoned.
    AccountClosed(AccountId),
    SameAccount,
    ZeroAmount,
    /// The account has an overdue debt: it cannot spend until it is solvent again.
    Blocked(AccountId),
    InsufficientFunds { needed: Amount, available: Amount },
    /// The common cash cannot finance the loan.
    BankIlliquid { needed: Amount, available: Amount },
    /// The mint has no LUX left for the registration grant (more must be emitted by vote).
    MintExhausted,
    CycleNotDue { next_at: Time },
    InvalidPayout(String),
    TimeWentBackwards { last: Time, got: Time },
}

impl fmt::Display for EconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use EconError::*;
        match self {
            UnknownAccount(a) => write!(f, "unknown account {a}"),
            AccountExists(a) => write!(f, "account {a} already exists"),
            AccountClosed(a) => write!(f, "account {a} is closed or abandoned"),
            SameAccount => write!(f, "buyer and seller are the same account"),
            ZeroAmount => write!(f, "amount must be greater than zero"),
            Blocked(a) => write!(f, "account {a} is blocked by an overdue debt"),
            InsufficientFunds { needed, available } => {
                write!(f, "insufficient funds: need {needed}, available (balance + credit) {available}")
            }
            BankIlliquid { needed, available } => write!(f, "the bank cannot finance {needed}: cash {available}"),
            MintExhausted => write!(f, "the mint has no LUX left; a vote must authorise a new emission"),
            CycleNotDue { next_at } => write!(f, "the next cycle closes at t={next_at}"),
            InvalidPayout(m) => write!(f, "invalid payout: {m}"),
            TimeWentBackwards { last, got } => write!(f, "time went backwards (last {last}, got {got})"),
        }
    }
}

impl std::error::Error for EconError {}
