// SPDX-License-Identifier: GPL-2.0-or-later
//! luxd: the openpolis node. First slice (M1): create the network, run the web API, issue PIN invitations.
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use lux_core::paths;
use lux_node::ddns::{self, Http, Step};
use lux_node::services::{ComposeProvisioner, ShellRunner};
use lux_node::{dns, serve, unix_now, InitParams, Node, NodeOptions};

#[derive(Parser)]
#[command(name = "luxd", version, about = "openpolis node")]
struct Cli {
    /// Data directory (default: $LUX_HOME or ~/.lux)
    #[arg(long, global = true)]
    home: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the network on this node: name, domain, currency and the first administrator.
    /// The password is read from $LUX_ADMIN_PASSWORD or asked on the terminal.
    Init {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "openpolis.site")]
        domain: String,
        #[arg(long, default_value = "Lux")]
        currency_name: String,
        #[arg(long, default_value = "LUX")]
        currency_symbol: String,
        #[arg(long)]
        admin_user: String,
        /// Services this node offers (default: all): subdomain, mail, chat, insta
        #[arg(long, value_delimiter = ',')]
        services: Option<Vec<String>>,
    },
    /// Serve the web pages and the API.
    Run {
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: String,
        /// Read the client address from X-Forwarded-For (only behind a proxy you control).
        #[arg(long)]
        trust_proxy: bool,
        /// Mark the session cookie as Secure (use it when the site is served over HTTPS).
        #[arg(long)]
        secure_cookies: bool,
        /// How accounts are created in the services: `noop` (they stay requested) or `compose` (docker exec on the
        /// containers of deploy/compose.yaml).
        #[arg(long, default_value = "noop")]
        provision: String,
    },
    /// Print the DNS records to add at the registrar.
    Dns {
        /// Public IPv4 address of this node
        #[arg(long)]
        ip: Option<String>,
        #[arg(long)]
        ipv6: Option<String>,
    },
    /// Keep the DNS pointing at this node when its public address changes (dynamic DNS).
    Ddns {
        /// URL that answers with the public address as plain text
        #[arg(long, default_value = "https://api.ipify.org")]
        ip_url: String,
        /// The provider's update URL, with {ip} where the address goes. Read from $OPENPOLIS_DDNS_URL if not given
        /// (it contains a secret: better not to leave it in the shell history).
        #[arg(long)]
        update_url: Option<String>,
        #[arg(long, default_value_t = 300)]
        interval_secs: u64,
        /// Check once and stop
        #[arg(long)]
        once: bool,
    },
    /// Invitation PINs.
    Pin {
        #[command(subcommand)]
        sub: PinCmd,
    },
    /// Show the network.
    Status,
    /// Verify the whole ledger (chain, hashes, signatures, rules) and print how many entries it has.
    Verify,
}

#[derive(Subcommand)]
enum PinCmd {
    /// Issue a new PIN (it is shown once).
    New {
        #[arg(long)]
        admin_user: String,
        #[arg(long, default_value_t = 24)]
        ttl_hours: i64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home = paths::home_dir(cli.home.as_deref());
    std::fs::create_dir_all(&home).with_context(|| format!("creating {}", home.display()))?;
    let db = paths::db_path(&home).to_string_lossy().into_owned();
    match cli.cmd {
        Cmd::Init { name, domain, currency_name, currency_symbol, admin_user, services } => {
            let password = match std::env::var("LUX_ADMIN_PASSWORD") {
                Ok(p) if !p.is_empty() => p,
                _ => {
                    let p = rpassword::prompt_password("Password dell'amministratore: ")?;
                    if p != rpassword::prompt_password("Ripetila: ")? {
                        bail!("le due password non coincidono");
                    }
                    p
                }
            };
            let node = Node::init(
                &db,
                InitParams { name, domain, currency_name, currency_symbol, admin_username: admin_user, admin_password: password },
                NodeOptions::default(),
                unix_now(),
            )?;
            if let Some(list) = services {
                node.set_services(&list)?;
            }
            let n = node.network_info()?;
            println!("Servizi offerti: {}", n.services.join(", "));
            println!("Rete «{}» creata su {} (moneta {} · {}).", n.name, n.domain, n.currency_name, n.currency_symbol);
            println!("Database: {db}");
            println!("Prossimo passo: `luxd run`, poi apri /admin per generare i PIN d'invito.");
        }
        Cmd::Run { listen, trust_proxy, secure_cookies, provision } => {
            let opts = NodeOptions { trust_proxy, secure_cookies, ..NodeOptions::default() };
            let mut node = Node::open(&db, opts)?;
            match provision.as_str() {
                "noop" => {}
                "compose" => node = node.with_provisioner(Arc::new(ComposeProvisioner::new(ShellRunner { timeout_secs: 90 }))),
                other => bail!("--provision: «{other}» non è noop né compose"),
            }
            let node = Arc::new(node);
            let h = serve(node.clone(), &listen).with_context(|| format!("cannot listen on {listen}"))?;
            println!("luxd in ascolto su http://{}  (Ctrl-C per fermare)", h.addr);
            // the parliament closes the votes whose time is up, and the bank closes the 28-day cycles, by themselves
            let bank = node.clone();
            std::thread::spawn(move || loop {
                match bank.close_due_proposals(unix_now()) {
                    Ok(0) => {}
                    Ok(n) => eprintln!("parlamento: chiuse {n} votazioni"),
                    Err(e) => eprintln!("parlamento: errore nella chiusura di una votazione: {e}"),
                }
                match bank.close_due_cycles(unix_now()) {
                    Ok(0) => {}
                    Ok(n) => eprintln!("banca: chiusi {n} cicli"),
                    Err(e) => eprintln!("banca: errore nella chiusura del ciclo: {e}"),
                }
                std::thread::sleep(std::time::Duration::from_secs(30));
            });
            h.wait();
        }
        Cmd::Pin { sub: PinCmd::New { admin_user, ttl_hours } } => {
            let node = Node::open(&db, NodeOptions::default())?;
            let p = node.create_pin_local(&admin_user, ttl_hours, unix_now())?;
            println!("PIN: {}", p.pin);
            println!("Vale {ttl_hours} ore e una sola volta. Consegnalo a mano o su un canale cifrato: non viene mostrato di nuovo.");
        }
        Cmd::Dns { ip, ipv6 } => {
            let n = Node::open(&db, NodeOptions::default())?.network_info()?;
            if ip.is_none() && ipv6.is_none() {
                bail!("indica l'indirizzo pubblico di questo nodo con --ip (e --ipv6 se ce l'hai)");
            }
            print!("{}", dns::table(&n.domain, &dns::records(&n.domain, ip.as_deref(), ipv6.as_deref())));
        }
        Cmd::Ddns { ip_url, update_url, interval_secs, once } => {
            let template = match update_url.or_else(|| std::env::var("OPENPOLIS_DDNS_URL").ok()) {
                Some(u) if !u.is_empty() => u,
                _ => bail!("serve l'indirizzo di aggiornamento del tuo fornitore DNS (--update-url o $OPENPOLIS_DDNS_URL)"),
            };
            let http = UreqHttp;
            let mut last = None;
            loop {
                match ddns::update_once(&http, &ip_url, &template, &mut last) {
                    Ok(Step::Updated(ip)) => println!("DNS aggiornato: {} → {ip}", ddns::host_of(&template)),
                    Ok(Step::Unchanged(_)) => {}
                    Err(e) => eprintln!("ddns: {e}"),
                }
                if once {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(interval_secs.max(30)));
            }
        }
        Cmd::Verify => {
            let n = Node::open(&db, NodeOptions::default())?; // opening already replays and verifies everything
            println!("Registro valido: {} voci.", n.verify_ledger()?);
        }
        Cmd::Status => {
            let n = Node::open(&db, NodeOptions::default())?.network_info()?;
            println!("{} · {} · moneta {} ({}) · {} membri", n.name, n.domain, n.currency_name, n.currency_symbol, n.members);
        }
    }
    Ok(())
}

/// The real HTTP client of the dynamic DNS (the logic and its tests are in lux-node).
struct UreqHttp;

impl Http for UreqHttp {
    fn get(&self, url: &str) -> Result<String, String> {
        let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(20)).build();
        match agent.get(url).call() {
            Ok(r) => r.into_string().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(code, _)) => Err(format!("risposta {code}")),
            Err(e) => Err(e.kind().to_string()),
        }
    }
}
