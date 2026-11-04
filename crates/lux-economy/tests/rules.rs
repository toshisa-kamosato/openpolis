// SPDX-License-Identifier: GPL-2.0-or-later
//! One test per rule of the openpolis economy (see docs/adr/0019-*.md).
use lux_economy::*;

const D: u64 = DAY;

fn lux(n: u64) -> Amount {
    Amount::lux(n)
}
fn net() -> Economy {
    Economy::new(Params::default(), 0)
}
fn net_with(params: Params, users: u64) -> Economy {
    let mut e = Economy::new(params, 0);
    for i in 1..=users {
        e.register(i, 0).unwrap();
    }
    e
}
fn users(n: u64) -> Economy {
    net_with(Params::default(), n)
}
fn ok(e: &Economy) {
    e.check_invariants().expect("invariants");
}
fn bal(e: &Economy, id: u64) -> Amount {
    e.account(id).unwrap().balance
}

#[test]
fn genesis_mint_cash_and_conservation() {
    let e = net();
    assert_eq!(e.emitted_total(), lux(1_000_000));
    assert_eq!(e.cash(), lux(5_000));
    assert_eq!(e.mint_balance(), lux(995_000));
    ok(&e);
}

#[test]
fn registration_funds_the_cash_and_accounts_start_at_zero() {
    let e = users(2);
    assert_eq!(e.cash(), lux(7_000));
    assert_eq!(e.mint_balance(), lux(993_000));
    assert_eq!(bal(&e, 1), Amount::ZERO);
    assert_eq!(e.account(1).unwrap().fido, Amount::ZERO, "no credit line before the first transaction");
    assert_eq!(e.members(), 2);
    ok(&e);
}

#[test]
fn registration_stops_when_the_mint_is_exhausted_until_a_vote_emits_more() {
    let mut p = Params::default();
    p.mint_initial = lux(6_000); // 5 000 seed + room for exactly one member
    let mut e = Economy::new(p, 0);
    e.register(1, 0).unwrap();
    assert_eq!(e.register(2, 0), Err(EconError::MintExhausted));
    e.mint_emit_by_vote(lux(1_000), 7, 10).unwrap();
    e.register(2, 11).unwrap();
    assert!(e.mint_log().iter().any(|m| m.reason == MintReason::Vote(7)));
    ok(&e);
}

#[test]
fn the_default_supply_supports_995_members() {
    let mut e = net();
    for i in 1..=995 {
        e.register(i, 0).unwrap();
    }
    assert_eq!(e.register(996, 0), Err(EconError::MintExhausted));
    ok(&e);
}

#[test]
fn first_transaction_grants_the_credit_line_and_splits_the_commission() {
    let mut e = users(2);
    let r = e.pay(1, 2, lux(50), 100).unwrap();
    assert_eq!(r.fee_buyer, Amount(125)); // 2.5 % of 50.00 = 1.25
    assert_eq!(r.fee_seller, Amount(125));
    assert_eq!(r.cost, Amount(5125)); // the buyer pays price + 2.5 %
    assert_eq!(r.seller_receives, Amount(4875)); // the seller receives price - 2.5 %
    assert_eq!(r.borrowed, Amount(5125)); // balance was 0: everything is financed by the bank
    assert_eq!(e.account(1).unwrap().fido, lux(100));
    assert_eq!(e.account(1).unwrap().first_tx, Some(100));
    assert_eq!(e.account(2).unwrap().fido, lux(100), "the seller's first transaction starts its credit line too");
    assert_eq!(bal(&e, 1), Amount::ZERO);
    assert_eq!(bal(&e, 2), Amount(4875));
    assert_eq!(e.cash(), Amount(695_125), "7000 - 51.25 lent + 2.50 commission");
    let d = e.debts_of(1);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].remaining, d[0].due), (Amount(5125), 100 + 28 * D));
    ok(&e);
}

#[test]
fn the_credit_line_is_checked_against_price_times_1_025() {
    let mut e = users(2);
    // 97.56 -> fee 2.44 -> cost 100.00: exactly the credit line
    assert!(e.pay(1, 2, Amount(9756), 0).is_ok());
    let mut e = users(2);
    // 97.57 -> fee 2.44 -> cost 100.01: one centi-LUX over
    assert_eq!(
        e.pay(1, 2, Amount(9757), 0),
        Err(EconError::InsufficientFunds { needed: Amount(10001), available: Amount(10000) })
    );
    assert_eq!(e.account(1).unwrap().first_tx, None, "a failed payment is not a first transaction");
    ok(&e);
}

#[test]
fn an_overdue_debt_blocks_the_account_and_incoming_lux_clear_the_oldest_debt_first() {
    let mut e = users(2);
    e.pay(1, 2, lux(20), 0).unwrap(); // debt 20.50 due at 28 days
    e.pay(1, 2, lux(20), D).unwrap(); // debt 20.50 due at 29 days
    let t = 28 * D + 1;
    assert!(e.is_blocked(1, t));
    assert_eq!(e.pay(1, 2, lux(1), t), Err(EconError::Blocked(1)));
    assert_eq!(e.transfer(1, 2, lux(1), t), Err(EconError::Blocked(1)));
    // a blocked account can still receive; the LUX it receives cover the debts, oldest first
    e.pay(2, 1, lux(26), t).unwrap(); // user 1 receives 26 - 0.65 = 25.35
    let d = e.debts_of(1);
    assert_eq!(d[0].remaining, Amount::ZERO);
    assert_eq!(d[0].paid_at, Some(t));
    assert_eq!(d[1].remaining, Amount(1565), "20.50 - the 4.85 left after the first debt");
    assert_eq!(bal(&e, 1), Amount::ZERO, "nothing is left over: everything went to the debts");
    assert!(!e.is_blocked(1, t), "solvent again: the remaining debt is not yet due");
    ok(&e);
}

#[test]
fn every_movement_between_users_pays_the_commission() {
    let mut e = users(2);
    e.pay(1, 2, lux(10), 0).unwrap(); // user 1 owes 10.25, user 2 holds 9.75
    let cash = e.cash();
    let b2 = bal(&e, 2);
    let r = e.transfer(2, 1, Amount(500), 1).unwrap();
    assert_eq!((r.fee_buyer, r.fee_seller), (Amount(13), Amount(13)), "2.5 % of 5.00 = 0.125 -> 0.13");
    assert_eq!(bal(&e, 2), b2 - Amount(513), "the sender pays amount + 2.5 %");
    // the receiver's 4.87 went straight to its debt, so the whole 5.13 ends up in the common cash
    assert_eq!(e.cash(), cash + Amount(513));
    assert_eq!(e.debt_total(1), Amount(1025 - 487));
    // a transfer can use the credit line like a payment, but not beyond it
    assert!(matches!(e.transfer(2, 1, lux(200), 2), Err(EconError::InsufficientFunds { .. })));
    ok(&e);
}

#[test]
fn selling_ones_credit_line_means_paying_beyond_the_balance_up_to_the_limit() {
    let mut e = users(2);
    let r = e.transfer(1, 2, lux(60), 0).unwrap();
    assert_eq!(r.borrowed, Amount(6150), "balance 0: the bank finances the whole 61.50");
    assert_eq!(e.debt_total(1), Amount(6150));
    assert_eq!(e.credit_available(1), Amount(3850));
    assert_eq!(
        e.transfer(1, 2, lux(40), 1),
        Err(EconError::InsufficientFunds { needed: Amount(4100), available: Amount(3850) })
    );
    ok(&e);
}

#[test]
fn three_solvent_cycles_raise_the_credit_line_to_300() {
    let mut e = users(2);
    e.pay(1, 2, lux(10), 0).unwrap(); // user 1 owes 10.25
    e.pay(2, 1, lux(11), 1).unwrap(); // user 1 receives 10.72: the debt is cleared
    assert_eq!(e.debt_total(1), Amount::ZERO);
    // three full cycles from the first transaction (t = 0) without any overdue debt
    e.pay(1, 2, Amount(100), 3 * 28 * D + 10).unwrap();
    let a = e.account(1).unwrap();
    assert_eq!(a.fido, lux(300));
    assert_eq!(a.cycles_done, 3);
    assert_eq!(e.credit_available(1), Amount(29944), "300.00 - the 0.56 just borrowed");
    ok(&e);
}

#[test]
fn an_overdue_debt_resets_the_streak() {
    let mut e = users(2);
    e.pay(1, 2, lux(5), 0).unwrap(); // user 1 owes 5.13 and does not pay it back
    // cycle 1 (days 0-28) is solvent; the debt falls overdue on day 28, inside cycle 2 (days 28-56)
    let t = 2 * 28 * D + 1;
    assert!(e.is_blocked(1, t));
    e.pay(2, 1, lux(6), t).unwrap(); // receiving is allowed even when blocked; cycles are settled here
    let a = e.account(1).unwrap();
    assert_eq!((a.cycles_done, a.solvent_streak, a.fido), (2, 0, lux(100)));
    assert!(!e.is_blocked(1, t), "the payment received cleared the debt");
    ok(&e);
}

#[test]
fn the_network_cycle_runs_28_days_from_creation() {
    let mut e = users(1);
    assert_eq!(e.next_cycle_at(), 28 * D);
    assert_eq!(e.close_cycle(28 * D - 1, &CycleInput::default()), Err(EconError::CycleNotDue { next_at: 28 * D }));
    let r1 = e.close_cycle(60 * D, &CycleInput::default()).unwrap();
    assert_eq!((r1.cycle, r1.boundary), (1, 28 * D));
    assert!(e.cycle_due(60 * D), "a second cycle is pending");
    let r2 = e.close_cycle(60 * D, &CycleInput::default()).unwrap();
    assert_eq!((r2.cycle, r2.boundary), (2, 56 * D));
    assert!(!e.cycle_due(60 * D));
    ok(&e);
}

#[test]
fn demurrage_taxes_only_the_excess_over_the_threshold_and_feeds_the_cash() {
    let mut p = Params::default();
    p.demurrage_threshold = lux(50);
    let mut e = net_with(p, 2);
    e.pay(1, 2, lux(60), 0).unwrap(); // user 2 receives 58.50
    let r = e.close_cycle(28 * D, &CycleInput::default()).unwrap();
    assert_eq!(r.demurrage, Amount(42), "5 % of (58.50 - 50.00), rounded down");
    // only user 2 is above the threshold: the tax went to the common cash (the refunds that follow are a separate flow)
    assert!(r.refunds_paid > Amount::ZERO);
    ok(&e);
}

#[test]
fn the_leftover_goes_back_to_users_in_proportion_to_the_commissions_they_paid() {
    let mut e = users(3);
    e.pay(1, 2, lux(40), D).unwrap(); // fees: user1 1.00, user2 1.00
    e.pay(2, 3, lux(80), D).unwrap(); // fees: user2 2.00, user3 2.00  -> totals 1 : 3 : 2
    // a refund first covers a member's debts, so the benefit is measured on balance minus debt
    let net = |e: &Economy, i: u64| bal(e, i).0 as i64 - e.debt_total(i).0 as i64;
    let before: Vec<i64> = (1..=3).map(|i| net(&e, i)).collect();
    let r = e.close_cycle(28 * D, &CycleInput::default()).unwrap();
    let d: Vec<i64> = (1..=3).map(|i| net(&e, i) - before[i as usize - 1]).collect();
    assert!(r.refunds_paid > Amount::ZERO);
    assert!((3 * d[0] - d[1]).abs() <= 3, "user2 paid 3x the fees of user1: {d:?}");
    assert!((2 * d[0] - d[2]).abs() <= 2, "user3 paid 2x the fees of user1: {d:?}");
    // the surplus is fully distributed except the rounding dust
    assert_eq!(e.summary().surplus, r.refund_dust.0 as i64);
    assert_eq!(r.refunds_paid + r.refund_dust, Amount(r.surplus_before as u64));
    ok(&e);
}

#[test]
fn works_and_expenses_follow_the_order_expenses_then_works_then_refunds() {
    let mut e = users(4);
    e.pay(1, 2, lux(40), D).unwrap();
    let input = CycleInput {
        network_expenses: vec![Payout { to: 3, amount: lux(100), memo: "server".into() }],
        works: vec![Payout { to: 4, amount: lux(500), memo: "playground".into() }],
    };
    let b3 = bal(&e, 3);
    let b4 = bal(&e, 4);
    let r = e.close_cycle(28 * D, &input).unwrap();
    assert_eq!(r.expenses_paid, lux(100));
    assert_eq!(r.works_paid, lux(500));
    assert_eq!(r.works_unpaid, Amount::ZERO);
    assert_eq!(bal(&e, 3) - b3, lux(100));
    assert_eq!(bal(&e, 4) - b4, lux(500));
    assert!(r.refunds_paid > Amount::ZERO, "something is left for the refunds");
    ok(&e);
}

#[test]
fn works_are_paid_only_from_the_real_surplus() {
    let mut p = Params::default();
    p.cash_seed = Amount::ZERO;
    p.cash_per_member = lux(120); // 2 users: cash 240, credit lines to cover about 200 -> a surplus of about 40
    let mut e = net_with(p, 2);
    e.pay(1, 2, Amount(100), 0).unwrap(); // starts both credit lines
    let input = CycleInput { network_expenses: vec![], works: vec![Payout { to: 2, amount: lux(1_000), memo: "big".into() }] };
    let r = e.close_cycle(28 * D, &input).unwrap();
    assert!(r.surplus_before > 0);
    assert_eq!(r.works_paid, Amount(r.surplus_before as u64), "all the surplus, not a centi-LUX more");
    assert_eq!(r.works_paid + r.works_unpaid, lux(1_000));
    assert_eq!(e.summary().surplus, 0, "the surplus is spent, not borrowed");
    ok(&e);
}

fn poor_bank(mint_initial: Amount) -> Economy {
    let mut p = Params::default();
    p.cash_seed = Amount::ZERO;
    p.cash_per_member = lux(20); // far less than the credit lines the users get
    p.mint_initial = mint_initial;
    net_with(p, 2)
}

#[test]
fn a_negative_surplus_suspends_works_and_refunds_and_takes_an_advance_from_the_mint() {
    let mut e = poor_bank(lux(1_000_000));
    e.pay(1, 2, lux(10), 0).unwrap(); // cash 40 - 10.25 + 0.50 = 30.25 ; undrawn lines 89.75 + 100 = 189.75
    let s = e.summary();
    assert_eq!(s.surplus, 3025 - 18975);
    let input = CycleInput { network_expenses: vec![], works: vec![Payout { to: 2, amount: lux(50), memo: "w".into() }] };
    let r = e.close_cycle(28 * D, &input).unwrap();
    assert_eq!(r.advance_taken, Amount(15950));
    assert_eq!(r.emergency_emitted, Amount::ZERO, "the mint had enough LUX");
    assert!(r.suspended);
    assert_eq!((r.works_paid, r.refunds_paid), (Amount::ZERO, Amount::ZERO));
    assert_eq!(e.advance_outstanding(), Amount(15950));
    assert_eq!(e.summary().surplus, 0);
    ok(&e);
}

#[test]
fn the_advance_is_repaid_first_with_the_surplus_net_of_network_expenses() {
    let mut e = poor_bank(lux(1_000_000));
    e.pay(1, 2, lux(10), 0).unwrap();
    e.close_cycle(28 * D, &CycleInput::default()).unwrap(); // advance 159.50
    // cycle 2: commissions on a 5.00 payment (0.13 + 0.13) are the only surplus
    e.pay(2, 1, lux(5), 29 * D).unwrap();
    let input = CycleInput {
        network_expenses: vec![],
        works: vec![Payout { to: 2, amount: lux(1), memo: "w".into() }],
    };
    let mint_before = e.mint_balance();
    let r = e.close_cycle(56 * D, &input).unwrap();
    assert_eq!(r.advance_repaid, Amount(26));
    assert_eq!(e.advance_outstanding(), Amount(15950 - 26));
    assert_eq!(e.mint_balance(), mint_before + Amount(26), "repaid LUX return to the mint");
    assert!(r.suspended, "no works or refunds until the advance is extinguished");
    assert_eq!(r.works_paid, Amount::ZERO);
    ok(&e);
}

#[test]
fn when_the_mint_is_short_it_emits_the_difference_for_the_advance() {
    let mut e = poor_bank(lux(100)); // after 2 registrations the mint holds only 60 LUX
    e.pay(1, 2, lux(10), 0).unwrap();
    let emitted = e.emitted_total();
    let r = e.close_cycle(28 * D, &CycleInput::default()).unwrap();
    assert_eq!(r.advance_taken, Amount(15950));
    assert_eq!(r.emergency_emitted, Amount(15950) - lux(60));
    assert_eq!(e.emitted_total(), emitted + r.emergency_emitted);
    assert!(e.mint_log().iter().any(|m| m.reason == MintReason::EmergencyAdvance));
    ok(&e);
}

#[test]
fn network_expenses_are_paid_even_when_the_surplus_is_negative() {
    let mut e = poor_bank(lux(1_000_000));
    e.pay(1, 2, lux(10), 0).unwrap();
    let input = CycleInput { network_expenses: vec![Payout { to: 2, amount: lux(5), memo: "bandwidth".into() }], works: vec![] };
    let b2 = bal(&e, 2);
    let r = e.close_cycle(28 * D, &input).unwrap();
    assert_eq!(r.expenses_paid, lux(5));
    assert_eq!(bal(&e, 2) - b2, lux(5));
    assert_eq!(r.advance_taken, Amount(15950) + lux(5), "the advance also covers what the expenses took out");
    assert_eq!(e.summary().surplus, 0);
    ok(&e);
}

#[test]
fn abandoned_accounts_have_their_uncovered_debts_cancelled_at_the_next_cycle_charged_to_the_surplus() {
    let run = |charge: bool| {
        let mut p = Params::default();
        p.charge_writeoffs_to_surplus = charge;
        let mut e = net_with(p, 2);
        e.pay(1, 2, lux(40), 0).unwrap(); // debt 41.00, balance 0
        e.mark_abandoned(1, 10 * D).unwrap();
        assert_eq!(e.pay(1, 2, lux(1), 11 * D), Err(EconError::AccountClosed(1)));
        assert_eq!(e.debt_total(1), lux(41), "nothing is cancelled before the cycle closes");
        let r = e.close_cycle(28 * D, &CycleInput::default()).unwrap();
        assert_eq!(r.writeoffs, lux(41));
        assert_eq!(r.closed_accounts, vec![1]);
        assert!(e.account(1).unwrap().closed);
        assert_eq!(e.debt_total(1), Amount::ZERO);
        ok(&e);
        r
    };
    let literal = run(true);
    let free = run(false);
    // the cancelled debt reduces what can be distributed in that cycle (the rest stays in the cash)
    assert_eq!(free.refunds_paid + free.refund_dust, Amount(free.surplus_before as u64));
    assert_eq!(literal.refunds_paid + literal.refund_dust, Amount(literal.surplus_before as u64) - lux(41));
    assert_eq!((literal.loss_target, literal.loss_held), (lux(41), lux(41)));
    assert_eq!((free.loss_target, free.loss_held), (Amount::ZERO, Amount::ZERO));
}

#[test]
fn a_large_writeoff_is_held_back_over_several_cycles_then_released() {
    let mut p = Params::default();
    p.cash_seed = Amount::ZERO;
    p.cash_per_member = lux(105);
    p.solvent_cycles_for_upgrade = 99; // keep the credit lines at 100 so the numbers below stay readable
    let mut e = net_with(p, 2);
    e.pay(1, 2, Amount(9756), 0).unwrap(); // cost 100.00: a debt of 100.00; commissions 2.44 + 2.44
    e.mark_abandoned(1, D).unwrap();
    let r1 = e.close_cycle(28 * D, &CycleInput::default()).unwrap();
    assert_eq!(r1.writeoffs, lux(100));
    assert_eq!(r1.surplus_before, 1488, "cash 114.88 - undrawn credit line of user 2: 100.00");
    assert_eq!((r1.loss_target, r1.loss_held), (lux(100), Amount(1488)), "the whole surplus is held back");
    assert!(r1.suspended);
    // fresh money arrives with a new member: the rest of the loss is held back, the surplus beyond it is free
    e.register(3, 29 * D).unwrap();
    let r2 = e.close_cycle(56 * D, &CycleInput::default()).unwrap();
    assert_eq!((r2.loss_target, r2.loss_held), (lux(100), lux(100)));
    assert_eq!(r2.refund_dust, Amount(1988), "119.88 of surplus - 100.00 held back");
    // the cycle after it is complete, what was held back returns to the surplus
    let r3 = e.close_cycle(84 * D, &CycleInput::default()).unwrap();
    assert_eq!((r3.loss_target, r3.loss_held), (Amount::ZERO, Amount::ZERO));
    assert_eq!(r3.refund_dust, Amount(11988));
    ok(&e);
}

#[test]
fn an_abandoned_wallet_goes_back_to_the_common_cash() {
    let mut e = users(2);
    e.pay(1, 2, lux(40), 0).unwrap(); // user 2 holds 39.00
    e.mark_abandoned(2, 2).unwrap();
    let r = e.close_cycle(28 * D, &CycleInput::default()).unwrap();
    assert_eq!(r.closed_accounts, vec![2]);
    assert_eq!(r.writeoffs, Amount::ZERO, "no debt: nothing to cancel");
    assert_eq!(bal(&e, 2), Amount::ZERO);
    ok(&e);
}

#[test]
fn operations_are_atomic_and_time_cannot_go_backwards() {
    let mut e = users(2);
    e.pay(1, 2, lux(10), 100).unwrap();
    let snapshot = e.clone();
    assert!(e.pay(1, 2, lux(10_000), 101).is_err());
    assert!(e.pay(1, 1, lux(1), 101).is_err());
    assert!(matches!(e.pay(1, 2, lux(1), 50), Err(EconError::TimeWentBackwards { .. })));
    assert_eq!(e, snapshot, "a failed operation leaves no trace");
}

#[test]
fn the_state_round_trips_through_json() {
    let mut e = users(3);
    e.pay(1, 2, lux(40), D).unwrap();
    e.close_cycle(28 * D, &CycleInput::default()).unwrap();
    let json = serde_json::to_string(&e).unwrap();
    let back: Economy = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e);
}
