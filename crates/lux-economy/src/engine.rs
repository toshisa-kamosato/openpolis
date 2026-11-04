// SPDX-License-Identifier: GPL-2.0-or-later
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::EconError;
use crate::money::Amount;
use crate::params::{Params, DAY};

pub type AccountId = u64;
pub type DebtId = u64;
/// Seconds since an arbitrary epoch chosen by the caller (the engine never reads a clock).
pub type Time = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub balance: Amount,
    /// Credit line. Zero until the first transaction.
    pub fido: Amount,
    /// Time of the first transaction: it starts the user's 28-day cycles.
    pub first_tx: Option<Time>,
    pub cycles_done: u64,
    pub solvent_streak: u32,
    pub abandoned: bool,
    pub closed: bool,
    pub registered_at: Time,
    pub debts: Vec<DebtId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debt {
    pub id: DebtId,
    pub user: AccountId,
    pub principal: Amount,
    pub remaining: Amount,
    pub created: Time,
    pub due: Time,
    pub paid_at: Option<Time>,
    pub written_off_at: Option<Time>,
}

impl Debt {
    fn is_open(&self) -> bool {
        !self.remaining.is_zero() && self.written_off_at.is_none()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MintReason {
    Genesis,
    /// Emission authorised by a vote (the proposal id belongs to the parliament module).
    Vote(u64),
    /// The mint had too few LUX for an extraordinary advance to the bank.
    EmergencyAdvance,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintEvent {
    pub time: Time,
    pub amount: Amount,
    pub reason: MintReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payout {
    pub to: AccountId,
    pub amount: Amount,
    pub memo: String,
}

/// What the network decided to spend this cycle. The engine enforces the order and the limits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleInput {
    /// Network running costs: paid first, always.
    pub network_expenses: Vec<Payout>,
    /// Social works approved by vote, in priority order: paid only from the real surplus.
    pub works: Vec<Payout>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayReceipt {
    pub price: Amount,
    pub fee_buyer: Amount,
    pub fee_seller: Amount,
    /// What the buyer paid: price + buyer fee.
    pub cost: Amount,
    /// What the seller received: price - seller fee.
    pub seller_receives: Amount,
    /// Part of `cost` financed by the bank (it becomes a debt).
    pub borrowed: Amount,
    pub debt: Option<DebtId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleReport {
    /// 1-based number of the cycle just closed.
    pub cycle: u64,
    pub boundary: Time,
    pub demurrage: Amount,
    /// Uncovered debts cancelled for abandoned accounts.
    pub writeoffs: Amount,
    pub closed_accounts: Vec<AccountId>,
    /// `cash - required` before paying anything (can be negative).
    pub surplus_before: i64,
    pub expenses_paid: Amount,
    pub advance_taken: Amount,
    /// Part of the advance that the mint had to emit because it did not hold enough LUX.
    pub emergency_emitted: Amount,
    pub advance_repaid: Amount,
    pub works_paid: Amount,
    pub works_unpaid: Amount,
    pub refunds_paid: Amount,
    /// Rounding remainder of the refunds, left in the cash.
    pub refund_dust: Amount,
    /// Works and refunds were not paid (negative surplus, advance outstanding, or write-offs absorbed it all).
    pub suspended: bool,
    pub advance_outstanding: Amount,
    /// Write-offs still being absorbed, and the part already held back in the cash.
    pub loss_target: Amount,
    pub loss_held: Amount,
}

/// What the user sees in the bank section of the wallet page.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankSummary {
    pub cash: Amount,
    /// Coverage kept in the cash: for each member, the credit line they have now minus their debts.
    pub required: Amount,
    pub surplus: i64,
    pub advance_outstanding: Amount,
    pub loss_target: Amount,
    pub loss_held: Amount,
    pub mint_balance: Amount,
    pub emitted_total: Amount,
    pub members: u64,
    pub credit_lines_total: Amount,
    pub open_debts_total: Amount,
    pub cycles_closed: u64,
    pub next_cycle_at: Time,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Economy {
    params: Params,
    created_at: Time,
    last_time: Time,
    mint_balance: Amount,
    emitted_total: Amount,
    mint_log: Vec<MintEvent>,
    cash: Amount,
    advance_outstanding: Amount,
    accounts: BTreeMap<AccountId, Account>,
    debts: BTreeMap<DebtId, Debt>,
    next_debt: DebtId,
    cycles_closed: u64,
    /// Commissions paid by each user during the current network cycle (basis of the refunds).
    fees_cycle: BTreeMap<AccountId, Amount>,
    members: u64,
    /// Cancelled debts still to be absorbed by the surplus (sum of write-offs not yet fully held back).
    #[serde(default)]
    loss_target: Amount,
    /// Part of `loss_target` already held back in the cash (not distributable). Released the cycle after it is complete.
    #[serde(default)]
    loss_held: Amount,
}

impl Economy {
    /// Creates the network: the mint emits `mint_initial` and moves `cash_seed` to the common cash.
    pub fn new(params: Params, now: Time) -> Economy {
        let mut e = Economy {
            params,
            created_at: now,
            last_time: now,
            mint_balance: Amount::ZERO,
            emitted_total: Amount::ZERO,
            mint_log: Vec::new(),
            cash: Amount::ZERO,
            advance_outstanding: Amount::ZERO,
            accounts: BTreeMap::new(),
            debts: BTreeMap::new(),
            next_debt: 1,
            cycles_closed: 0,
            fees_cycle: BTreeMap::new(),
            members: 0,
            loss_target: Amount::ZERO,
            loss_held: Amount::ZERO,
        };
        let init = e.params.mint_initial;
        e.emit(init, MintReason::Genesis, now);
        let seed = e.params.cash_seed.min(e.mint_balance);
        e.mint_balance -= seed;
        e.cash += seed;
        e
    }

    // ------------------------------------------------------------------ read access
    pub fn params(&self) -> &Params {
        &self.params
    }
    pub fn cash(&self) -> Amount {
        self.cash
    }
    pub fn mint_balance(&self) -> Amount {
        self.mint_balance
    }
    pub fn emitted_total(&self) -> Amount {
        self.emitted_total
    }
    pub fn advance_outstanding(&self) -> Amount {
        self.advance_outstanding
    }
    pub fn members(&self) -> u64 {
        self.members
    }
    pub fn cycles_closed(&self) -> u64 {
        self.cycles_closed
    }
    pub fn mint_log(&self) -> &[MintEvent] {
        &self.mint_log
    }
    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.get(&id)
    }
    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }
    pub fn debts_of(&self, id: AccountId) -> Vec<&Debt> {
        match self.accounts.get(&id) {
            Some(a) => a.debts.iter().filter_map(|d| self.debts.get(d)).collect(),
            None => Vec::new(),
        }
    }
    pub fn debt_total(&self, id: AccountId) -> Amount {
        self.debts_of(id).iter().filter(|d| d.is_open()).fold(Amount::ZERO, |s, d| s + d.remaining)
    }
    /// Credit line still available to the user.
    pub fn credit_available(&self, id: AccountId) -> Amount {
        match self.accounts.get(&id) {
            Some(a) => a.fido.saturating_sub(self.debt_total(id)),
            None => Amount::ZERO,
        }
    }
    /// An overdue debt blocks the account (it can still receive and repay).
    pub fn is_blocked(&self, id: AccountId, now: Time) -> bool {
        self.debts_of(id).iter().any(|d| d.is_open() && now > d.due)
    }
    pub fn next_cycle_at(&self) -> Time {
        self.created_at + (self.cycles_closed + 1) * self.params.cycle_days * DAY
    }
    pub fn cycle_due(&self, now: Time) -> bool {
        now >= self.next_cycle_at()
    }
    /// What the common cash must be able to honour: for each member the credit line they have *now* (100 LUX for
    /// the first three solvent cycles, then 300) minus their debts. Members who have not transacted yet have no
    /// credit line and need no coverage.
    pub fn required(&self) -> Amount {
        self.accounts.values().filter(|a| !a.closed).fold(Amount::ZERO, |s, a| s + a.fido.saturating_sub(self.debt_total(a.id)))
    }
    pub fn summary(&self) -> BankSummary {
        let required = self.required();
        BankSummary {
            cash: self.cash,
            required,
            surplus: self.cash.0 as i64 - required.0 as i64,
            advance_outstanding: self.advance_outstanding,
            loss_target: self.loss_target,
            loss_held: self.loss_held,
            mint_balance: self.mint_balance,
            emitted_total: self.emitted_total,
            members: self.members,
            credit_lines_total: self.accounts.values().filter(|a| !a.closed).fold(Amount::ZERO, |s, a| s + a.fido),
            open_debts_total: self.debts.values().filter(|d| d.is_open()).fold(Amount::ZERO, |s, d| s + d.remaining),
            cycles_closed: self.cycles_closed,
            next_cycle_at: self.next_cycle_at(),
        }
    }

    // ------------------------------------------------------------------ operations (all atomic)
    /// Registers a member: the mint moves `cash_per_member` to the common cash. The balance starts at 0.
    pub fn register(&mut self, id: AccountId, now: Time) -> Result<(), EconError> {
        self.atomically(|e| {
            e.tick(now)?;
            if e.accounts.contains_key(&id) {
                return Err(EconError::AccountExists(id));
            }
            let grant = e.params.cash_per_member;
            if e.mint_balance < grant {
                return Err(EconError::MintExhausted);
            }
            e.mint_balance -= grant;
            e.cash += grant;
            e.accounts.insert(
                id,
                Account {
                    id,
                    balance: Amount::ZERO,
                    fido: Amount::ZERO,
                    first_tx: None,
                    cycles_done: 0,
                    solvent_streak: 0,
                    abandoned: false,
                    closed: false,
                    registered_at: now,
                    debts: Vec::new(),
                },
            );
            e.members += 1;
            Ok(())
        })
    }

    /// A payment (or any movement between users): the sender pays `price + fee`, the receiver gets `price - fee`.
    /// The credit line is checked against `price × 1.025` (the buyer's total cost).
    pub fn pay(&mut self, buyer: AccountId, seller: AccountId, price: Amount, now: Time) -> Result<PayReceipt, EconError> {
        self.atomically(|e| {
            e.tick(now)?;
            if buyer == seller {
                return Err(EconError::SameAccount);
            }
            if price.is_zero() {
                return Err(EconError::ZeroAmount);
            }
            e.require_open(buyer)?;
            e.require_open(seller)?;
            e.begin_tx(buyer, now);
            e.begin_tx(seller, now);
            if e.is_blocked(buyer, now) {
                return Err(EconError::Blocked(buyer));
            }
            let fee_buyer = price.bps_round(e.params.fee_buyer_bps);
            let fee_seller = price.bps_round(e.params.fee_seller_bps);
            let cost = price + fee_buyer;
            let seller_receives = price.saturating_sub(fee_seller);
            let balance = e.accounts[&buyer].balance;
            let credit = e.credit_available(buyer);
            if cost > balance + credit {
                return Err(EconError::InsufficientFunds { needed: cost, available: balance + credit });
            }
            let borrowed = cost.saturating_sub(balance);
            let debt = if borrowed.is_zero() { None } else { Some(e.lend(buyer, borrowed, now)?) };
            e.accounts.get_mut(&buyer).unwrap().balance -= cost;
            e.credit(seller, seller_receives, now);
            e.cash += (cost - price) + (price - seller_receives);
            *e.fees_cycle.entry(buyer).or_insert(Amount::ZERO) += fee_buyer;
            *e.fees_cycle.entry(seller).or_insert(Amount::ZERO) += fee_seller;
            Ok(PayReceipt { price, fee_buyer, fee_seller, cost, seller_receives, borrowed, debt })
        })
    }

    /// Any movement of LUX between two users. It pays the same commission as a payment: there is no way to
    /// move LUX between users for free (the credit line can be used, as in a payment).
    pub fn transfer(&mut self, from: AccountId, to: AccountId, amount: Amount, now: Time) -> Result<PayReceipt, EconError> {
        self.pay(from, to, amount, now)
    }

    /// Marks an account as abandoned: it stops operating and, at the next cycle, the bank cancels its uncovered debts.
    pub fn mark_abandoned(&mut self, user: AccountId, now: Time) -> Result<(), EconError> {
        self.atomically(|e| {
            e.tick(now)?;
            e.require_open(user)?;
            e.accounts.get_mut(&user).unwrap().abandoned = true;
            Ok(())
        })
    }

    /// A new emission authorised by a vote (the parliament module checks the vote; the engine records it).
    pub fn mint_emit_by_vote(&mut self, amount: Amount, proposal: u64, now: Time) -> Result<(), EconError> {
        self.atomically(|e| {
            e.tick(now)?;
            if amount.is_zero() {
                return Err(EconError::ZeroAmount);
            }
            e.emit(amount, MintReason::Vote(proposal), now);
            Ok(())
        })
    }

    /// Closes the oldest pending network cycle. Call it again if `cycle_due` is still true.
    pub fn close_cycle(&mut self, now: Time, input: &CycleInput) -> Result<CycleReport, EconError> {
        self.atomically(|e| {
            e.tick(now)?;
            let boundary = e.next_cycle_at();
            if now < boundary {
                return Err(EconError::CycleNotDue { next_at: boundary });
            }
            e.close_cycle_inner(now, boundary, input)
        })
    }

    // ------------------------------------------------------------------ internals
    fn atomically<T>(&mut self, f: impl FnOnce(&mut Economy) -> Result<T, EconError>) -> Result<T, EconError> {
        let mut next = self.clone();
        let out = f(&mut next)?;
        *self = next;
        Ok(out)
    }

    fn tick(&mut self, now: Time) -> Result<(), EconError> {
        if now < self.last_time {
            return Err(EconError::TimeWentBackwards { last: self.last_time, got: now });
        }
        self.last_time = now;
        Ok(())
    }

    fn emit(&mut self, amount: Amount, reason: MintReason, now: Time) {
        self.emitted_total += amount;
        self.mint_balance += amount;
        self.mint_log.push(MintEvent { time: now, amount, reason });
    }

    fn require_open(&self, id: AccountId) -> Result<(), EconError> {
        match self.accounts.get(&id) {
            None => Err(EconError::UnknownAccount(id)),
            Some(a) if a.closed || a.abandoned => Err(EconError::AccountClosed(id)),
            Some(_) => Ok(()),
        }
    }

    /// Starts the user's cycles and credit line at the first transaction, then settles completed cycles.
    fn begin_tx(&mut self, id: AccountId, now: Time) {
        let initial = self.params.fido_initial;
        let a = self.accounts.get_mut(&id).expect("account checked by the caller");
        if a.first_tx.is_none() {
            a.first_tx = Some(now);
            a.fido = initial;
        }
        self.process_user_cycles(id, now);
    }

    /// For every 28-day cycle of the user that has ended: a cycle without overdue debts is *solvent*.
    /// After `solvent_cycles_for_upgrade` solvent cycles in a row the credit line rises; an insolvent cycle
    /// resets the streak and brings the credit line back to the initial amount.
    fn process_user_cycles(&mut self, id: AccountId, now: Time) {
        let len = self.params.cycle_days * DAY;
        loop {
            let (t0, done, closed) = {
                let a = &self.accounts[&id];
                match a.first_tx {
                    Some(t) => (t, a.cycles_done, a.closed),
                    None => return,
                }
            };
            if closed {
                return;
            }
            let start = t0 + done * len;
            let end = start + len;
            if end > now {
                return;
            }
            let solvent = !self.had_overdue_in(id, start, end);
            let (need, trusted, initial) =
                (self.params.solvent_cycles_for_upgrade, self.params.fido_trusted, self.params.fido_initial);
            let a = self.accounts.get_mut(&id).unwrap();
            if solvent {
                a.solvent_streak += 1;
                if a.solvent_streak >= need {
                    a.fido = trusted;
                }
            } else {
                a.solvent_streak = 0;
                a.fido = initial;
            }
            a.cycles_done += 1;
        }
    }

    /// Was any debt of the user overdue at some instant in `[start, end)`?
    fn had_overdue_in(&self, id: AccountId, start: Time, end: Time) -> bool {
        self.debts_of(id).iter().any(|d| {
            let until = d.paid_at.or(d.written_off_at).unwrap_or(u64::MAX);
            let lo = (d.due + 1).max(start);
            let hi = until.min(end);
            lo < hi
        })
    }

    /// The bank finances `amount` from the common cash: balance up, debt created.
    fn lend(&mut self, user: AccountId, amount: Amount, now: Time) -> Result<DebtId, EconError> {
        if self.cash < amount {
            return Err(EconError::BankIlliquid { needed: amount, available: self.cash });
        }
        self.cash -= amount;
        self.accounts.get_mut(&user).unwrap().balance += amount;
        let id = self.next_debt;
        self.next_debt += 1;
        let due = now + self.params.debt_term_days * DAY;
        self.debts.insert(
            id,
            Debt { id, user, principal: amount, remaining: amount, created: now, due, paid_at: None, written_off_at: None },
        );
        self.accounts.get_mut(&user).unwrap().debts.push(id);
        Ok(id)
    }

    /// LUX arriving in an account (a sale, a transfer from another user, a payout of the bank). A member in debt
    /// first covers the debts, oldest first, and only the rest raises the balance: a debt is cleared by a transaction
    /// with other users (already taxed), never by a direct payment to the bank.
    fn credit(&mut self, user: AccountId, amount: Amount, now: Time) {
        self.accounts.get_mut(&user).unwrap().balance += amount;
        let owed = self.debt_total(user);
        if !owed.is_zero() {
            let bal = self.accounts[&user].balance;
            self.apply_repay(user, bal.min(owed), now);
        }
    }

    /// FIFO repayment: oldest debt first. The caller has checked that `amount <= balance`.
    fn apply_repay(&mut self, user: AccountId, amount: Amount, now: Time) -> Amount {
        let ids: Vec<DebtId> = self.accounts[&user].debts.clone();
        let mut left = amount;
        let mut repaid = Amount::ZERO;
        for id in ids {
            if left.is_zero() {
                break;
            }
            let d = self.debts.get_mut(&id).unwrap();
            if !d.is_open() {
                continue;
            }
            let pay = d.remaining.min(left);
            d.remaining -= pay;
            if d.remaining.is_zero() {
                d.paid_at = Some(now);
            }
            left -= pay;
            repaid += pay;
        }
        self.accounts.get_mut(&user).unwrap().balance -= repaid;
        self.cash += repaid;
        repaid
    }

    /// Moves `amount` from the mint to the common cash, emitting what the mint lacks.
    fn take_from_mint(&mut self, amount: Amount, now: Time) -> Amount {
        let mut emergency = Amount::ZERO;
        if self.mint_balance < amount {
            emergency = amount - self.mint_balance;
            self.emit(emergency, MintReason::EmergencyAdvance, now);
        }
        self.mint_balance -= amount;
        self.cash += amount;
        emergency
    }

    fn close_cycle_inner(&mut self, now: Time, boundary: Time, input: &CycleInput) -> Result<CycleReport, EconError> {
        // 1. settle every user's own cycles up to now
        let ids: Vec<AccountId> = self.accounts.keys().copied().collect();
        for id in &ids {
            self.process_user_cycles(*id, now);
        }

        // 2. demurrage on balances above the threshold -> common cash
        let mut demurrage = Amount::ZERO;
        let (thr, dbps) = (self.params.demurrage_threshold, self.params.demurrage_bps);
        for id in &ids {
            let a = self.accounts.get_mut(id).unwrap();
            if a.closed || a.abandoned || a.balance <= thr {
                continue;
            }
            let tax = (a.balance - thr).bps_floor(dbps);
            a.balance -= tax;
            demurrage += tax;
        }
        self.cash += demurrage;

        // 3. abandoned accounts: own LUX cover the debts first, the rest is cancelled, the account closes
        let mut writeoffs = Amount::ZERO;
        let mut closed_accounts = Vec::new();
        for id in &ids {
            if !(self.accounts[id].abandoned && !self.accounts[id].closed) {
                continue;
            }
            let bal = self.accounts[id].balance;
            if !bal.is_zero() && !self.debt_total(*id).is_zero() {
                self.apply_repay(*id, bal.min(self.debt_total(*id)), now);
            }
            let debt_ids: Vec<DebtId> = self.accounts[id].debts.clone();
            for d in debt_ids {
                let debt = self.debts.get_mut(&d).unwrap();
                if debt.is_open() {
                    writeoffs += debt.remaining;
                    debt.written_off_at = Some(now);
                }
            }
            let a = self.accounts.get_mut(id).unwrap();
            self.cash += a.balance; // what is left of an abandoned wallet goes back to the common cash
            a.balance = Amount::ZERO;
            a.fido = Amount::ZERO;
            a.closed = true;
            closed_accounts.push(*id);
        }

        // 4. recipients of the payouts must be able to receive (this starts their credit line if they are new)
        for p in input.network_expenses.iter().chain(input.works.iter()) {
            if p.amount.is_zero() {
                return Err(EconError::InvalidPayout("zero amount".into()));
            }
            self.require_open(p.to).map_err(|_| EconError::InvalidPayout(format!("recipient {} cannot receive", p.to)))?;
            self.begin_tx(p.to, now);
        }

        let surplus_before = self.cash.0 as i64 - self.required().0 as i64;

        // 5. network expenses are paid first and always
        let expenses_total = input.network_expenses.iter().fold(Amount::ZERO, |s, p| s + p.amount);
        if self.cash < expenses_total {
            return Err(EconError::BankIlliquid { needed: expenses_total, available: self.cash });
        }
        for p in &input.network_expenses {
            self.cash -= p.amount;
            self.credit(p.to, p.amount, now);
        }

        let mut rep = CycleReport {
            cycle: self.cycles_closed + 1,
            boundary,
            demurrage,
            writeoffs,
            closed_accounts,
            surplus_before,
            expenses_paid: expenses_total,
            advance_taken: Amount::ZERO,
            emergency_emitted: Amount::ZERO,
            advance_repaid: Amount::ZERO,
            works_paid: Amount::ZERO,
            works_unpaid: input.works.iter().fold(Amount::ZERO, |s, p| s + p.amount),
            refunds_paid: Amount::ZERO,
            refund_dust: Amount::ZERO,
            suspended: false,
            advance_outstanding: Amount::ZERO,
            loss_target: Amount::ZERO,
            loss_held: Amount::ZERO,
        };

        // cancelled debts: what was held back earlier and is now complete returns to the surplus (it is released
        // the cycle after it is complete); the new write-offs join what is still to be absorbed
        if self.params.charge_writeoffs_to_surplus {
            if !self.loss_target.is_zero() && self.loss_held >= self.loss_target {
                self.loss_held = Amount::ZERO;
                self.loss_target = Amount::ZERO;
            }
            self.loss_target += writeoffs;
        }

        // 6. liquidity: the cash must again cover the undrawn credit lines
        let after_expenses = self.cash.0 as i64 - self.required().0 as i64;
        if after_expenses < 0 {
            let advance = Amount((-after_expenses) as u64);
            rep.emergency_emitted = self.take_from_mint(advance, now);
            self.advance_outstanding += advance;
            rep.advance_taken = advance;
            rep.suspended = true;
            self.loss_held = Amount::ZERO; // no surplus, nothing can be held back
        } else {
            let mut avail = Amount(after_expenses as u64);
            // the surplus first returns to the mint, until the advance is extinguished
            let back = avail.min(self.advance_outstanding);
            if !back.is_zero() {
                self.cash -= back;
                self.mint_balance += back;
                self.advance_outstanding -= back;
                avail -= back;
                rep.advance_repaid = back;
            }
            if !self.advance_outstanding.is_zero() {
                rep.suspended = true;
                self.loss_held = Amount::ZERO;
            } else {
                // hold back, from the fresh surplus, what is still missing to absorb the cancelled debts
                self.loss_held = self.loss_held.min(avail);
                let mut free = avail - self.loss_held;
                let take = free.min(self.loss_target - self.loss_held);
                self.loss_held += take;
                free -= take;
                avail = free;
                if avail.is_zero() && (!input.works.is_empty() || !self.fees_cycle.is_empty()) {
                    rep.suspended = true;
                }
                // social works voted by the network, in order
                for p in &input.works {
                    let pay = p.amount.min(avail);
                    if pay.is_zero() {
                        continue;
                    }
                    self.cash -= pay;
                    self.credit(p.to, pay, now);
                    avail -= pay;
                    rep.works_paid += pay;
                }
                rep.works_unpaid = rep.works_unpaid - rep.works_paid;
                // what is left goes back to the users in proportion to the commissions they paid
                let total_fees = self.fees_cycle.values().fold(Amount::ZERO, |s, f| s + *f);
                let mut paid = Amount::ZERO;
                if !avail.is_zero() && !total_fees.is_zero() {
                    let shares: Vec<(AccountId, Amount)> = self
                        .fees_cycle
                        .iter()
                        .map(|(u, f)| (*u, Amount(((avail.0 as u128 * f.0 as u128) / total_fees.0 as u128) as u64)))
                        .collect();
                    for (u, share) in shares {
                        if share.is_zero() || self.accounts[&u].closed {
                            continue;
                        }
                        self.cash -= share;
                        self.credit(u, share, now);
                        paid += share;
                    }
                }
                rep.refunds_paid = paid;
                rep.refund_dust = avail - paid;
            }
        }

        self.fees_cycle.clear();
        self.cycles_closed += 1;
        rep.advance_outstanding = self.advance_outstanding;
        rep.loss_target = self.loss_target;
        rep.loss_held = self.loss_held;
        Ok(rep)
    }

    // ------------------------------------------------------------------ self-check
    /// Conservation and consistency checks. Used by the tests; cheap enough to call in production after each cycle.
    pub fn check_invariants(&self) -> Result<(), String> {
        let wallets: u64 = self.accounts.values().map(|a| a.balance.0).sum();
        let total = self.mint_balance.0 + self.cash.0 + wallets;
        if total != self.emitted_total.0 {
            return Err(format!("supply not conserved: mint {} + cash {} + wallets {} != emitted {}",
                self.mint_balance, self.cash, Amount(wallets), self.emitted_total));
        }
        let minted: u64 = self.mint_log.iter().map(|m| m.amount.0).sum();
        if minted != self.emitted_total.0 {
            return Err("mint log does not match emitted total".into());
        }
        if self.advance_outstanding > self.emitted_total {
            return Err("advance larger than the supply".into());
        }
        if self.loss_held > self.loss_target || self.loss_held > self.cash {
            return Err(format!("inconsistent loss accounting: held {} target {} cash {}", self.loss_held, self.loss_target, self.cash));
        }
        for a in self.accounts.values() {
            if a.closed && (!a.balance.is_zero() || !a.fido.is_zero() || !self.debt_total(a.id).is_zero()) {
                return Err(format!("closed account {} is not empty", a.id));
            }
            if !a.closed && !a.balance.is_zero() && !self.debt_total(a.id).is_zero() {
                return Err(format!("account {} has a positive balance and a debt at the same time", a.id));
            }
            if a.first_tx.is_none() && (!a.fido.is_zero() || !a.debts.is_empty()) {
                return Err(format!("account {} has credit before its first transaction", a.id));
            }
            for d in &a.debts {
                match self.debts.get(d) {
                    Some(x) if x.user == a.id && x.remaining <= x.principal => {}
                    _ => return Err(format!("inconsistent debt {d} of account {}", a.id)),
                }
            }
        }
        for d in self.debts.values() {
            if !self.accounts.contains_key(&d.user) {
                return Err(format!("debt {} has no owner", d.id));
            }
            if d.is_open() && d.paid_at.is_some() {
                return Err(format!("debt {} is open but marked paid", d.id));
            }
        }
        for u in self.fees_cycle.keys() {
            if !self.accounts.contains_key(u) {
                return Err(format!("fees recorded for unknown account {u}"));
            }
        }
        Ok(())
    }
}
