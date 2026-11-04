// SPDX-License-Identifier: GPL-2.0-or-later
//! Networking layer of luxd. See `docs/adr/0002-*.md` for the model (untrusted nodes, distributed from day one).
#![forbid(unsafe_code)]

mod identity;
mod node;

pub use identity::load_or_create_identity;
pub use node::{spawn, InboxMsg, NodeConfig, NodeHandle, PeerInfo, StatusInfo};
pub use libp2p::{Multiaddr, PeerId};
