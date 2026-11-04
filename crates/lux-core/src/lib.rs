// SPDX-License-Identifier: GPL-2.0-or-later
//! Shared building blocks: where things live on disk, the daemon configuration and the
//! JSON-lines control protocol spoken over the local Unix socket.
#![forbid(unsafe_code)]

pub mod config;
pub mod ctl;
pub mod paths;
