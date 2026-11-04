// SPDX-License-Identifier: GPL-2.0-or-later
use crate::money::Amount;
use serde::{Deserialize, Serialize};

pub const DAY: u64 = 86_400;

/// Network parameters. The defaults are the openpolis rules; each network can change them only by a vote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Params {
    /// Length of a cycle (both the per-user cycle and the network/bank cycle).
    pub cycle_days: u64,
    /// A debt must be repaid within this many days of the transaction that created it.
    pub debt_term_days: u64,
    /// Commission charged to the buyer on top of the price, in basis points (250 = 2.5 %).
    pub fee_buyer_bps: u32,
    /// Commission deducted from what the seller receives, in basis points.
    pub fee_seller_bps: u32,
    /// Credit line granted at a user's first transaction.
    pub fido_initial: Amount,
    /// Credit line after `solvent_cycles_for_upgrade` consecutive solvent cycles.
    pub fido_trusted: Amount,
    pub solvent_cycles_for_upgrade: u32,
    /// Balances above this pay `demurrage_bps` on the excess, at every network cycle.
    pub demurrage_threshold: Amount,
    pub demurrage_bps: u32,
    /// LUX emitted by the mint when the network is created.
    pub mint_initial: Amount,
    /// Moved from the mint to the common cash at creation.
    pub cash_seed: Amount,
    /// Moved from the mint to the common cash for each registered member.
    pub cash_per_member: Amount,
    /// Charge cancelled debts against the cycle's distributable surplus (see ADR-0019).
    pub charge_writeoffs_to_surplus: bool,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            cycle_days: 28,
            debt_term_days: 28,
            fee_buyer_bps: 250,
            fee_seller_bps: 250,
            fido_initial: Amount::lux(100),
            fido_trusted: Amount::lux(300),
            solvent_cycles_for_upgrade: 3,
            demurrage_threshold: Amount::lux(300),
            demurrage_bps: 500,
            mint_initial: Amount::lux(1_000_000),
            cash_seed: Amount::lux(5_000),
            cash_per_member: Amount::lux(1_000),
            charge_writeoffs_to_surplus: true,
        }
    }
}
