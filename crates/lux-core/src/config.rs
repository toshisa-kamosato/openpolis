// SPDX-License-Identifier: GPL-2.0-or-later
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Daemon configuration (`config.toml`). Every field has a default so an empty file is valid.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Multiaddrs to listen on.
    pub listen: Vec<String>,
    /// Multiaddrs (including `/p2p/<peer id>`) of nodes to join the network through.
    pub bootstrap: Vec<String>,
    /// Act as a circuit-relay server (for the public gateway / always-on nodes).
    pub relay_server: bool,
    /// Discover peers on the local network.
    pub mdns: bool,
    /// Gossip topic used for network-wide announcements.
    pub topic: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            listen: vec!["/ip4/0.0.0.0/tcp/4001".into(), "/ip4/0.0.0.0/udp/4001/quic-v1".into()],
            bootstrap: vec![],
            relay_server: false,
            mdns: true,
            topic: "lux/announce/v1".into(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).context("serializing config")?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn round_trip() {
        let mut c = Config::default();
        c.bootstrap.push("/ip4/1.2.3.4/tcp/4001/p2p/12D3KooWExample".into());
        c.relay_server = true;
        let text = toml::to_string_pretty(&c).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), c);
    }
}
