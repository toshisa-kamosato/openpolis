// SPDX-License-Identifier: GPL-2.0-or-later
//! The node of an openpolis network (first slice, milestone M1): the network record (name, domain, currency),
//! the administrator, PIN invitations (ADR-0010) and member sign-up, behind a small local web API.
#![forbid(unsafe_code)]

mod auth;
pub mod ddns;
pub mod dns;
mod error;
mod http;
mod ledger;
mod node;
mod pages;
pub mod services;

pub use error::NodeError;
pub use http::{serve, ServerHandle};
pub use node::{unix_now, BankView, InitParams, LedgerLine, MyServices, NetworkInfo, NewPin, Node, NodeOptions, PayOutcome, PinInfo, ProposalView, ServiceStatus, UserInfo, VoteLine, WalletView};
