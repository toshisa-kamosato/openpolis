// SPDX-License-Identifier: GPL-2.0-or-later
//! Randomised simulation: thousands of operations in many networks, checking after EVERY step that
//! supply is conserved, the books are consistent, and a failed operation leaves the state untouched.
use lux_economy::*;

const D: u64 = DAY;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

#[derive(Default, Debug)]
struct Stats {
    ops_ok: u64,
    ops_err: u64,
    cycles: u64,
    advances: u64,
    advance_total: u64,
    emergencies: u64,
    writeoffs: u64,
    refunds: u64,
    works: u64,
    blocked_ops: u64,
    peak_debt: u64,
}

fn pick_open(e: &Economy, rng: &mut Rng, n: u64) -> Option<u64> {
    if n == 0 {
        return None;
    }
    for _ in 0..8 {
        let id = 1 + rng.below(n);
        if matches!(e.account(id), Some(a) if !a.closed && !a.abandoned) {
            return Some(id);
        }
    }
    None
}

fn run(seed: u64, params: Params, users: u64, steps: u64) -> Stats {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut e = Economy::new(params, 0);
    let mut t: Time = 0;
    let mut registered = 0u64;
    let mut st = Stats::default();

    for _ in 0..steps {
        t += rng.below(3 * D / 2);

        // close every pending cycle, with random expenses and works
        while e.cycle_due(t) {
            let mut input = CycleInput::default();
            if rng.below(2) == 0 {
                if let Some(u) = pick_open(&e, &mut rng, registered) {
                    input.network_expenses.push(Payout { to: u, amount: Amount(100 + rng.below(5_000)), memo: "net".into() });
                }
            }
            if rng.below(2) == 0 {
                if let Some(u) = pick_open(&e, &mut rng, registered) {
                    input.works.push(Payout { to: u, amount: Amount(100 + rng.below(30_000)), memo: "work".into() });
                }
            }
            let before = e.clone();
            let r = match e.close_cycle(t, &input) {
                Ok(r) => r,
                Err(_) => {
                    assert_eq!(e, before, "a failed close_cycle left a trace");
                    // closing with nothing to pay must always work
                    e.close_cycle(t, &CycleInput::default()).expect("empty cycle must close")
                }
            };
            e.check_invariants().unwrap_or_else(|m| panic!("seed {seed}, after cycle {}: {m}", r.cycle));
            st.cycles += 1;
            st.advance_total += r.advance_taken.0;
            st.advances += (!r.advance_taken.is_zero()) as u64;
            st.emergencies += (!r.emergency_emitted.is_zero()) as u64;
            st.writeoffs += r.writeoffs.0;
            st.refunds += r.refunds_paid.0;
            st.works += r.works_paid.0;
        }

        let before = e.clone();
        let roll = rng.below(100);
        let res: Result<(), EconError> = if roll < 50 {
            match (pick_open(&e, &mut rng, registered), pick_open(&e, &mut rng, registered)) {
                (Some(b), Some(s)) => e.pay(b, s, Amount(1 + rng.below(12_000)), t).map(|_| ()),
                _ => Ok(()),
            }
        } else if roll < 80 {
            match (pick_open(&e, &mut rng, registered), pick_open(&e, &mut rng, registered)) {
                (Some(a), Some(b)) => e.transfer(a, b, Amount(1 + rng.below(8_000)), t).map(|_| ()),
                _ => Ok(()),
            }
        } else if roll < 82 {
            match pick_open(&e, &mut rng, registered) {
                Some(u) => e.mark_abandoned(u, t),
                None => Ok(()),
            }
        } else if roll < 84 {
            e.mint_emit_by_vote(Amount(100 + rng.below(100_000)), rng.below(1_000), t)
        } else if registered < users {
            registered += 1;
            e.register(registered, t)
        } else {
            Ok(())
        };
        match res {
            Ok(()) => st.ops_ok += 1,
            Err(err) => {
                st.ops_err += 1;
                st.blocked_ops += matches!(err, EconError::Blocked(_)) as u64;
                if matches!(err, EconError::MintExhausted) {
                    registered -= 1;
                }
                assert_eq!(e, before, "seed {seed}: a failed operation ({err}) left a trace");
            }
        }
        e.check_invariants().unwrap_or_else(|m| panic!("seed {seed}: {m}"));
        st.peak_debt = st.peak_debt.max(e.summary().open_debts_total.0);
    }
    st
}

fn stress() -> Params {
    let mut p = Params::default();
    p.cash_seed = Amount::ZERO;
    p.cash_per_member = Amount::lux(25); // a bank that cannot honour its credit lines: advances and emergencies
    p.mint_initial = Amount::lux(450); // 15 members take 375: the mint cannot fund a real advance, so it must emit
    p
}

#[test]
fn default_rules_hold_for_many_random_networks() {
    for seed in 1..=60 {
        run(seed, Params::default(), 15, 500);
    }
}

#[test]
fn a_poorly_funded_bank_keeps_the_books_straight_through_advances_and_emergency_emissions() {
    let mut any_advance = false;
    let mut any_emergency = false;
    for seed in 1..=60 {
        let s = run(seed, stress(), 15, 500);
        any_advance |= s.advances > 0;
        any_emergency |= s.emergencies > 0;
    }
    assert!(any_advance, "the stress runs should trigger at least one advance");
    assert!(any_emergency, "...and at least one emergency emission");
}

#[test]
fn the_literal_and_the_free_writeoff_rules_both_conserve_supply() {
    let mut p = Params::default();
    p.charge_writeoffs_to_surplus = false;
    for seed in 100..130 {
        run(seed, p.clone(), 12, 400);
    }
}

#[test]
fn print_one_run_for_the_record() {
    let s = run(7, Params::default(), 15, 800);
    eprintln!("default rules, seed 7: {s:#?}");
    let s = run(7, stress(), 15, 800);
    eprintln!("stress rules,  seed 7: {s:#?}");
}
