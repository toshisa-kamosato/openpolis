// SPDX-License-Identifier: GPL-2.0-or-later
//! The economic engine of an openpolis network: **mint** (zecca), **common cash** (cassa comune) and **bank** (banca),
//! with credit lines (fidi), 28-day cycles, debts, commissions, demurrage and the extraordinary advance.
//!
//! The engine is a deterministic state machine: no clock, no I/O, no randomness. Every operation takes the current
//! time as an argument and is **atomic** (on error the state is left untouched). Amounts are integers in
//! *centi-LUX* (two decimals). The rules and the interpretations adopted are written in `docs/adr/0019-*.md`.
#![forbid(unsafe_code)]

mod engine;
mod error;
mod money;
mod params;

pub use engine::{
    Account, AccountId, BankSummary, CycleInput, CycleReport, Debt, DebtId, Economy, MintEvent, MintReason, PayReceipt,
    Payout, Time,
};
pub use error::EconError;
pub use money::Amount;
pub use params::{Params, DAY};
