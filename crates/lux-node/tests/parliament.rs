// SPDX-License-Identifier: GPL-2.0-or-later
//! The parliament (ADR-0021): proposals, open ballots, quorum and majority, and the effects of an accepted proposal.
mod common;
use common::*;
use lux_node::{Node, NodeError, NodeOptions};

const MINT: u64 = 500_00;

#[test]
fn a_text_proposal_is_voted_and_accepted() {
    let mut n = Net::new();
    let mut ms = n.members(5);
    let id = n.propose(&mut ms[0], "text", "Piantiamo un albero", 0, "", 3).unwrap();
    n.vote_all(&mut ms, id, &["yes", "yes", "no", "abstain", ""]);
    let v = n.view(&ms[4], id);
    assert_eq!((v.status.as_str(), v.yes, v.no, v.abstain, v.eligible, v.quorum_needed), ("open", 2, 1, 1, 5, 1));
    assert_eq!(v.votes.len(), 4, "the ballot is open: everyone sees who voted what");
    assert!(v.can_vote && v.my_vote.is_none());
    assert_eq!(n.node.close_due_proposals(n.now + 2 * D).unwrap(), 0, "not before the deadline");
    n.advance(3);
    assert_eq!(n.node.close_due_proposals(n.now).unwrap(), 1);
    assert_eq!(n.node.close_due_proposals(n.now).unwrap(), 0, "once");
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    let v = n.view(&ms[4], id);
    assert_eq!((v.status.as_str(), v.yes, v.no, v.abstain), ("accepted", 2, 1, 1));
    assert!(!v.can_vote);
    assert_eq!(v.proposer, "mem1");
    n.node.verify_ledger().unwrap();
}

#[test]
fn quorum_is_required_even_for_a_unanimous_vote() {
    let mut n = Net::new();
    let mut ms = n.members(6);
    let id = n.propose(&mut ms[0], "text", "Poco seguita", 0, "", 1).unwrap();
    n.vote(&mut ms[0], id, "yes").unwrap();
    assert_eq!(n.view(&ms[0], id).quorum_needed, 2, "20 % of 6, rounded up");
    n.advance(2);
    n.node.close_due_proposals(n.now).unwrap();
    n.relogin(&mut ms[0]);
    assert_eq!(n.view(&ms[0], id).status, "no_quorum");
}

#[test]
fn a_tie_is_not_a_majority_and_abstentions_count_for_the_quorum_only() {
    let mut n = Net::new();
    let mut ms = n.members(5);
    let id = n.propose(&mut ms[0], "text", "Pareggio", 0, "", 1).unwrap();
    n.vote_all(&mut ms, id, &["yes", "yes", "no", "no", "abstain"]);
    let id2 = n.propose(&mut ms[1], "text", "Solo astenuti", 0, "", 1).unwrap();
    n.vote_all(&mut ms, id2, &["abstain", "abstain", "abstain", "", ""]);
    n.advance(2);
    n.node.close_due_proposals(n.now).unwrap();
    n.relogin(&mut ms[0]);
    assert_eq!(n.view(&ms[0], id).status, "rejected");
    assert_eq!(n.view(&ms[0], id2).status, "rejected", "no yes at all");
}

#[test]
fn only_members_who_had_their_wallet_before_the_proposal_can_vote() {
    let mut n = Net::new();
    let mut ms = n.members(2);
    let late = n.join("tarde", 9); // joined, but no wallet yet
    let id = n.propose(&mut ms[0], "text", "Prima", 0, "", 5).unwrap();
    n.make_wallet(&late);
    let mut late = late;
    assert!(matches!(n.vote(&mut late, id, "yes"), Err(NodeError::Conflict(_))), "wallet created after the proposal");
    assert!(matches!(n.propose(&mut late, "text", "Anche io", 0, "", 5), Ok(_)), "but they can propose");
    assert_eq!(n.view(&ms[0], id).eligible, 2);
    // somebody with no wallet at all can neither propose nor vote
    let mut nobody = n.join("senzachiave", 10);
    assert!(matches!(n.propose(&mut nobody, "text", "x", 0, "", 5), Err(NodeError::Conflict(_))));
    assert!(matches!(n.vote(&mut nobody, id, "yes"), Err(NodeError::Conflict(_))));
}

#[test]
fn a_vote_can_be_changed_until_the_deadline_and_not_after() {
    let mut n = Net::new();
    let mut ms = n.members(3);
    let id = n.propose(&mut ms[0], "text", "Cambio idea", 0, "", 2).unwrap();
    n.vote(&mut ms[1], id, "yes").unwrap();
    n.vote(&mut ms[1], id, "no").unwrap();
    let v = n.view(&ms[1], id);
    assert_eq!((v.yes, v.no, v.my_vote.as_deref()), (0, 1, Some("no")), "the last vote counts");
    assert!(matches!(n.vote(&mut ms[1], id, "maybe"), Err(NodeError::InvalidInput(_))));
    assert!(matches!(n.vote(&mut ms[1], 999, "yes"), Err(NodeError::InvalidInput(_))), "no such proposal");
    n.advance(2); // the deadline has passed, the node has not closed the proposal yet
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert!(matches!(n.vote(&mut ms[1], id, "yes"), Err(NodeError::Conflict(_))));
    n.node.close_due_proposals(n.now).unwrap();
    assert!(matches!(n.vote(&mut ms[2], id, "yes"), Err(NodeError::Conflict(_))), "closed");
}

#[test]
fn what_can_be_proposed() {
    let mut n = Net::new();
    let mut ms = n.members(2);
    let inv = |r: Result<i64, NodeError>| matches!(r, Err(NodeError::InvalidInput(_)));
    assert!(inv(n.propose(&mut ms[0], "text", "x", 0, "", 0)), "0 days");
    assert!(inv(n.propose(&mut ms[0], "text", "x", 0, "", 29)), "more than 28 days");
    assert!(inv(n.propose(&mut ms[0], "text", "   ", 0, "", 3)), "no title");
    assert!(inv(n.propose(&mut ms[0], "text", &"x".repeat(121), 0, "", 3)), "title too long");
    assert!(inv(n.propose(&mut ms[0], "text", "x", 100, "", 3)), "a text proposal has no amount");
    assert!(inv(n.propose(&mut ms[0], "mint", "x", 0, "", 3)), "an emission needs an amount");
    assert!(inv(n.propose(&mut ms[0], "mint", "x", MINT, "mem2", 3)), "an emission has no recipient");
    assert!(inv(n.propose(&mut ms[0], "mint", "x", 1_000_000_00 + 1, "", 3)), "more than the whole supply");
    assert!(inv(n.propose(&mut ms[0], "works", "x", MINT, "", 3)), "a work needs a recipient");
    assert!(inv(n.propose(&mut ms[0], "works", "x", MINT, "nessuno", 3)), "recipient must be a member");
    assert!(inv(n.propose(&mut ms[0], "expense", "x", 0, "mem2", 3)));
    assert!(inv(n.propose(&mut ms[0], "boh", "x", 0, "", 3)), "unknown type");
    assert!(inv(n.propose(&mut ms[0], "abandon", "x", 0, "mem2", 3)), "mem2 has just joined: not quiet for 84 days");
    assert_eq!(n.node.proposals(&ms[0].token, n.now).unwrap().len(), 0, "nothing was written");
    // at most 3 open proposals per member
    for i in 0..3 {
        n.propose(&mut ms[0], "text", &format!("p{i}"), 0, "", 3).unwrap();
    }
    assert!(matches!(n.propose(&mut ms[0], "text", "p4", 0, "", 3), Err(NodeError::Conflict(_))));
    n.propose(&mut ms[1], "text", "altro membro", 0, "", 3).unwrap();
}

#[test]
fn an_accepted_emission_creates_money_and_a_rejected_one_does_not() {
    let mut n = Net::new();
    let mut ms = n.members(3);
    let ok = n.propose(&mut ms[0], "mint", "Più moneta", MINT, "", 1).unwrap();
    let no = n.propose(&mut ms[1], "mint", "Ancora più moneta", MINT * 2, "", 1).unwrap();
    n.vote_all(&mut ms, ok, &["yes", "yes", "no"]);
    n.vote_all(&mut ms, no, &["no", "no", "yes"]);
    let before = n.node.wallet(&ms[0].token, n.now).unwrap().bank;
    n.advance(2);
    assert_eq!(n.node.close_due_proposals(n.now).unwrap(), 2);
    n.relogin(&mut ms[0]);
    let after = n.node.wallet(&ms[0].token, n.now).unwrap().bank;
    assert_eq!(after.emitted_total, before.emitted_total + MINT, "only the accepted emission");
    assert_eq!(after.mint_balance, before.mint_balance + MINT);
    assert_eq!((n.view(&ms[0], ok).status.as_str(), n.view(&ms[0], no).status.as_str()), ("accepted", "rejected"));
    n.node.verify_ledger().unwrap();
}

#[test]
fn an_accepted_expense_and_an_accepted_work_are_paid_by_the_next_cycle() {
    let mut n = Net::new();
    let mut ms = n.members(4);
    let e = n.propose(&mut ms[0], "expense", "Il server", 50_00, "mem3", 1).unwrap();
    let w = n.propose(&mut ms[1], "works", "Il parco giochi", 100_00, "mem4", 1).unwrap();
    let rejected = n.propose(&mut ms[2], "works", "Una fontana", 70_00, "mem2", 1).unwrap();
    n.vote_all(&mut ms, e, &["yes", "yes", "yes", ""]);
    n.vote_all(&mut ms, w, &["yes", "yes", "no", ""]);
    n.vote_all(&mut ms, rejected, &["no", "no", "yes", ""]);
    n.advance(2);
    n.node.close_due_proposals(n.now).unwrap();
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert_eq!((n.balance(&ms[2]), n.balance(&ms[3]), n.balance(&ms[1])), (0, 0, 0), "nothing is paid until the cycle closes");
    n.advance(27); // day 29: the first cycle is due
    assert_eq!(n.node.close_due_cycles(n.now).unwrap(), 1);
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert_eq!(n.balance(&ms[2]), 50_00, "the approved expense, in full");
    assert_eq!(n.balance(&ms[3]), 100_00, "the approved work, from the surplus");
    assert_eq!(n.balance(&ms[1]), 0, "the rejected one is not paid");
    // and only once
    n.advance(28);
    n.node.close_due_cycles(n.now).unwrap();
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert_eq!((n.balance(&ms[2]), n.balance(&ms[3])), (50_00, 100_00));
    n.node.verify_ledger().unwrap();
}

#[test]
fn a_work_bigger_than_the_surplus_is_carried_over_to_the_next_cycles() {
    let path = tmp("carry");
    let mut n = Net::on(&path);
    let mut ms = n.members(4);
    let big = 900_000_00; // far more than the common cash
    let w = n.propose(&mut ms[0], "works", "Una grande opera", big, "mem2", 1).unwrap();
    n.vote_all(&mut ms, w, &["yes", "yes", "yes", "yes"]);
    n.advance(2);
    n.node.close_due_proposals(n.now).unwrap();
    n.advance(27);
    n.node.close_due_cycles(n.now).unwrap();
    n.advance(28);
    n.node.close_due_cycles(n.now).unwrap();
    n.node.verify_ledger().unwrap();
    drop(n);
    // the second cycle paid what was left of the first: read both cycle entries
    let db = rusqlite::Connection::open(&path).unwrap();
    let inputs: Vec<serde_json::Value> = {
        let mut st = db.prepare("SELECT payload FROM ledger WHERE kind = 'close_cycle' ORDER BY seq").unwrap();
        let rows = st.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(|p| serde_json::from_str::<serde_json::Value>(&p.unwrap()).unwrap()["input"].clone()).collect()
    };
    assert_eq!(inputs.len(), 2);
    let first = inputs[0]["works"][0]["amount"].as_u64().unwrap();
    let second = inputs[1]["works"][0]["amount"].as_u64().unwrap();
    assert_eq!(first, big);
    assert!(second < big && second > 0, "the second cycle owes what the first could not pay: {second}");
    // the node restarts and the queue is rebuilt from the ledger
    let node = Node::open(&path, NodeOptions::default()).unwrap();
    assert!(node.verify_ledger().unwrap() > 0);
}

#[test]
fn the_parliament_survives_a_restart() {
    let path = tmp("restart");
    let id;
    {
        let mut n = Net::on(&path);
        let mut ms = n.members(3);
        id = n.propose(&mut ms[0], "works", "Un lavoro", 10_00, "mem2", 1).unwrap();
        n.vote_all(&mut ms, id, &["yes", "yes", "no"]);
        n.advance(2);
        n.node.close_due_proposals(n.now).unwrap();
    }
    let node = Node::open(&path, NodeOptions::default()).unwrap();
    let t = node.login("mem3", PW, IP, T0 + 3 * D).unwrap();
    let v = node.proposals(&t, T0 + 3 * D).unwrap().into_iter().find(|p| p.id == id).unwrap();
    assert_eq!((v.status.as_str(), v.yes, v.no, v.title.as_str()), ("accepted", 2, 1, "Un lavoro"));
    // the approved work is still waiting for the cycle
    node.close_due_cycles(T0 + 29 * D).unwrap();
    let t2 = node.login("mem2", PW, IP, T0 + 29 * D).unwrap();
    assert_eq!(node.wallet(&t2, T0 + 29 * D).unwrap().balance, 10_00);
}

#[test]
fn abandonment_is_decided_by_vote_and_only_for_members_who_have_gone_quiet() {
    let mut n = Net::new();
    let mut ms = n.members(4);
    n.pay(&mut ms[0], "mem2", 40_00).unwrap(); // mem1 owes 41.00 and then goes quiet
    let inv = |r: Result<i64, NodeError>| matches!(r, Err(NodeError::InvalidInput(_)));
    assert!(inv(n.propose(&mut ms[1], "abandon", "mem1 se n'è andato", 0, "mem1", 3)), "mem1 has just been active");
    n.advance(90);
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert!(inv(n.propose(&mut ms[1], "abandon", "x", 0, "mem2", 3)), "not oneself");
    assert!(inv(n.propose(&mut ms[1], "abandon", "x", 0, "nessuno", 3)), "not a stranger");
    assert!(inv(n.propose(&mut ms[1], "abandon", "x", 5_00, "mem1", 3)), "no amount");
    let id = n.propose(&mut ms[1], "abandon", "mem1 è sparito", 0, "mem1", 3).unwrap();
    assert!(matches!(n.propose(&mut ms[2], "abandon", "ancora", 0, "mem1", 3), Err(NodeError::Conflict(_))), "one open proposal per member");
    n.vote_all(&mut ms, id, &["", "yes", "yes", "no"]);
    n.advance(4);
    assert_eq!(n.node.close_due_proposals(n.now).unwrap(), 1);
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert_eq!(n.view(&ms[1], id).status, "accepted");
    n.node.close_due_cycles(n.now).unwrap();
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert!(matches!(n.pay(&mut ms[0], "mem2", 1_00), Err(NodeError::InvalidInput(_))), "the account is closed");
    assert!(matches!(n.pay(&mut ms[1], "mem1", 1_00), Err(NodeError::InvalidInput(_))), "nobody can pay a closed account");
    n.node.verify_ledger().unwrap();
}

#[test]
fn an_abandonment_is_void_if_the_member_comes_back_before_the_vote_ends() {
    let mut n = Net::new();
    let mut ms = n.members(4);
    n.advance(90);
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    let id = n.propose(&mut ms[1], "abandon", "mem1 è sparito?", 0, "mem1", 3).unwrap();
    n.vote_all(&mut ms, id, &["no", "yes", "yes", "yes"]); // mem1 votes: it is not gone
    n.advance(4);
    n.node.close_due_proposals(n.now).unwrap();
    for m in ms.iter_mut() {
        n.relogin(m);
    }
    assert_eq!(n.view(&ms[0], id).status, "void", "accepted by the votes, void because mem1 signed something");
    n.pay(&mut ms[0], "mem2", 1_00).unwrap(); // the account is alive
}

#[test]
fn an_emission_of_money_needs_two_thirds_while_other_decisions_need_a_simple_majority() {
    let mut n = Net::new();
    let mut ms = n.members(6);
    let text = n.propose(&mut ms[0], "text", "Decisione", 0, "", 1).unwrap();
    let mint_a = n.propose(&mut ms[1], "mint", "Emissione A", MINT, "", 1).unwrap();
    let mint_b = n.propose(&mut ms[2], "mint", "Emissione B", MINT, "", 1).unwrap();
    n.vote_all(&mut ms, text, &["yes", "yes", "yes", "no", "no", "abstain"]);
    n.vote_all(&mut ms, mint_a, &["yes", "yes", "yes", "yes", "no", "no"]); // exactly 2/3 of yes+no
    n.vote_all(&mut ms, mint_b, &["yes", "yes", "yes", "no", "no", "abstain"]); // 3 of 5: more than half, less than 2/3
    assert_eq!(n.view(&ms[0], mint_a).needs, "two_thirds");
    assert_eq!(n.view(&ms[0], text).needs, "simple");
    n.advance(2);
    n.node.close_due_proposals(n.now).unwrap();
    n.relogin(&mut ms[0]);
    let st = |id| n.view(&ms[0], id).status;
    assert_eq!((st(text).as_str(), st(mint_a).as_str(), st(mint_b).as_str()), ("accepted", "accepted", "rejected"));
}

// ---------------------------------------------------------------- the node cannot cheat
fn scenario(name: &str) -> (String, Vec<Member>, i64) {
    let path = tmp(name);
    let mut n = Net::on(&path);
    let mut ms = n.members(4);
    let id = n.propose(&mut ms[0], "text", "Pareggio", 0, "", 1).unwrap();
    n.vote_all(&mut ms, id, &["yes", "yes", "no", "no"]);
    (path, ms, id)
}

#[test]
fn the_node_cannot_mint_money_without_a_vote() {
    let (path, _, _) = scenario("forge-mint");
    let db = rusqlite::Connection::open(&path).unwrap();
    raw_node_entry(&db, "mint_vote", last_time(&db), r#"{"amount":99999999,"proposal":1}"#);
    assert!(open_err(&path).contains("tipo di voce sconosciuto"));
}

#[test]
fn the_node_cannot_claim_a_result_the_votes_do_not_support() {
    let (path, _, id) = scenario("forge-close");
    let db = rusqlite::Connection::open(&path).unwrap();
    let t = last_time(&db) + 2 * D;
    raw_node_entry(&db, "proposal_close", t, &format!(r#"{{"proposal":{id},"yes":2,"no":2,"abstain":0,"eligible":4,"outcome":"accepted"}}"#));
    assert!(open_err(&path).contains("l'esito dichiarato non corrisponde ai voti"));
}

#[test]
fn the_node_cannot_close_a_vote_early() {
    let (path, _, id) = scenario("forge-early");
    let db = rusqlite::Connection::open(&path).unwrap();
    let t = last_time(&db);
    raw_node_entry(&db, "proposal_close", t, &format!(r#"{{"proposal":{id},"yes":2,"no":2,"abstain":0,"eligible":4,"outcome":"rejected"}}"#));
    assert!(open_err(&path).contains("chiusura prima della scadenza"));
}

#[test]
fn the_node_cannot_make_a_cycle_pay_what_nobody_voted() {
    let (path, ms, _) = scenario("forge-cycle");
    let db = rusqlite::Connection::open(&path).unwrap();
    let t = last_time(&db) + 30 * D;
    let to = ms[3].name.clone();
    let id: i64 = db.query_row("SELECT json_extract(payload, '$.account') FROM ledger WHERE kind = 'register' AND json_extract(payload, '$.username') = ?1", [&to], |r| r.get(0)).unwrap();
    raw_node_entry(&db, "close_cycle", t, &format!(r#"{{"input":{{"network_expenses":[],"works":[{{"to":{id},"amount":123456,"memo":"regalo"}}]}}}}"#));
    assert!(open_err(&path).contains("l'ingresso del ciclo non corrisponde alle proposte approvate"));
}

#[test]
fn a_vote_cannot_be_slipped_in_after_the_deadline() {
    use ed25519_dalek::Signer;
    let (path, ms, id) = scenario("forge-late-vote");
    let db = rusqlite::Connection::open(&path).unwrap();
    let t = last_time(&db) + 2 * D; // after the deadline of 1 day
    let net_id: String = serde_json::from_str::<serde_json::Value>(&db.query_row::<String, _, _>("SELECT payload FROM ledger WHERE seq = 1", [], |r| r.get(0)).unwrap()).unwrap()["network_id"].as_str().unwrap().to_string();
    let voter = &ms[0]; // m1 holds a valid key, and signs correctly
    let uid: i64 = db.query_row("SELECT id FROM users WHERE username = 'mem1'", [], |r| r.get(0)).unwrap();
    let payload = serde_json::json!({"v":1,"net":net_id,"kind":"vote","from":"mem1","proposal":id,"choice":"yes","nonce":voter.next,"t":t}).to_string();
    let sig = hex(&voter.key.sign(payload.as_bytes()).to_bytes());
    raw_append(&db, "vote", t, &payload, &format!("user:{uid}"), &sig);
    assert!(open_err(&path).contains("la votazione è chiusa"));
}
