// SPDX-License-Identifier: GPL-2.0-or-later
//! `lux`: command line for the local luxd daemon.
#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use lux_core::config::Config;
use lux_core::ctl::{self, Request};
use lux_core::paths;

#[derive(Parser)]
#[command(name = "lux", version, about = "LUX: talk to your local luxd node")]
struct Cli {
    /// Data directory (default: $LUX_HOME or ~/.lux)
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the data directory, the node identity and a default config.toml
    Init {
        #[arg(long)]
        bootstrap: Vec<String>,
        #[arg(long)]
        relay_server: bool,
    },
    /// Print this node's PeerId
    Id,
    /// Show node status
    Status,
    /// List connected peers
    Peers,
    /// Connect to a peer (multiaddr)
    Dial { addr: String },
    /// Publish a message to the network announce topic
    Say { text: Vec<String> },
    /// Show messages received on the announce topic
    Inbox,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let home = paths::home_dir(cli.home.as_deref());
    let sock = paths::socket_path(&home);
    match cli.cmd {
        Cmd::Init { bootstrap, relay_server } => {
            std::fs::create_dir_all(&home)?;
            let id = lux_net::load_or_create_identity(&paths::identity_path(&home))?;
            let cfg_path = paths::config_path(&home);
            if !cfg_path.exists() {
                let mut cfg = Config::default();
                cfg.bootstrap = bootstrap;
                cfg.relay_server = relay_server;
                cfg.save(&cfg_path)?;
                println!("config written to {}", cfg_path.display());
            }
            println!("home:    {}", home.display());
            println!("peer id: {}", id.public().to_peer_id());
        }
        Cmd::Id => {
            let id = lux_net::load_or_create_identity(&paths::identity_path(&home))?;
            println!("{}", id.public().to_peer_id());
        }
        Cmd::Status => {
            let d = data(&sock, Request::Status).await?;
            println!("peer id:     {}", d["peer_id"].as_str().unwrap_or("?"));
            println!("agent:       {}", d["agent"].as_str().unwrap_or("?"));
            println!("peers:       {}", d["connected_peers"]);
            println!("nat:         {}", d["nat"].as_str().unwrap_or("?"));
            println!("relay srv:   {}", d["relay_server"]);
            for k in ["listen", "external", "relay_reservations"] {
                for v in d[k].as_array().into_iter().flatten() {
                    println!("{k:<12} {}", v.as_str().unwrap_or("?"));
                }
            }
        }
        Cmd::Peers => {
            let d = data(&sock, Request::Peers).await?;
            let list = d.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                println!("no peers connected");
            }
            for p in list {
                let relayed = if p["relayed"].as_bool().unwrap_or(false) { " (relayed)" } else { "" };
                println!("{}  {}{}", p["peer_id"].as_str().unwrap_or("?"), p["agent"].as_str().unwrap_or(""), relayed);
                for a in p["addrs"].as_array().into_iter().flatten() {
                    println!("    {}", a.as_str().unwrap_or("?"));
                }
            }
        }
        Cmd::Dial { addr } => {
            data(&sock, Request::Dial { addr }).await?;
            println!("dialing...");
        }
        Cmd::Say { text } => {
            data(&sock, Request::Say { text: text.join(" ") }).await?;
            println!("published");
        }
        Cmd::Inbox => {
            let d = data(&sock, Request::Inbox).await?;
            let list = d.as_array().cloned().unwrap_or_default();
            if list.is_empty() {
                println!("(empty)");
            }
            for m in list {
                println!("[{}] {}: {}", m["at"], m["from"].as_str().unwrap_or("?"), m["text"].as_str().unwrap_or(""));
            }
        }
    }
    Ok(())
}

async fn data(sock: &std::path::Path, req: Request) -> Result<serde_json::Value> {
    let r = ctl::call(sock, &req).await?;
    if !r.ok {
        bail!("{}", r.error.unwrap_or_else(|| "unknown error".into()));
    }
    Ok(r.data)
}
