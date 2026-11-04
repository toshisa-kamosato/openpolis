// SPDX-License-Identifier: GPL-2.0-or-later
//! User services (ADR-0022): which services exist on a node, and how a member's accounts are created in them.
//!
//! The node never keeps a member's password: it receives it at sign-up (or when the member asks for a service later),
//! hands it to a `Provisioner` that creates the accounts, and forgets it. What is stored is only the *state* of each
//! service for each member (requested / active / failed) and a human-readable detail.
use std::fmt;
use std::sync::Mutex;

/// Services a node can offer. `wallet` and `parliament` are always there and are not in this list.
pub const CATALOG: [&str; 4] = ["subdomain", "mail", "chat", "insta"];

pub fn label(service: &str) -> &'static str {
    match service {
        "subdomain" => "Il tuo sottodominio",
        "mail" => "Posta elettronica",
        "chat" => "Chat (Matrix)",
        "insta" => "Blog e galleria",
        _ => "?",
    }
}

/// Checks a choice of services against what the node offers. Returns the services in catalogue order, once each.
pub fn validate(requested: &[String], offered: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for s in CATALOG {
        if requested.iter().any(|r| r == s) {
            if !offered.iter().any(|o| o == s) {
                return Err(format!("Il servizio «{s}» non è offerto da questa rete."));
            }
            out.push(s.to_string());
        }
    }
    if let Some(bad) = requested.iter().find(|r| !CATALOG.contains(&r.as_str())) {
        return Err(format!("Servizio sconosciuto: «{bad}»."));
    }
    if out.iter().any(|s| s == "insta") && !out.iter().any(|s| s == "subdomain") {
        return Err("Il blog vive nel tuo sottodominio: scegli anche il sottodominio.".into());
    }
    Ok(out)
}

pub struct ProvisionRequest {
    pub username: String,
    pub domain: String,
    /// The member's password: used once, never stored, never logged.
    pub password: String,
    pub services: Vec<String>,
    pub mail_quota: &'static str,
    pub blog_quota_mb: u64,
}

impl fmt::Debug for ProvisionRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProvisionRequest").field("username", &self.username).field("domain", &self.domain).field("services", &self.services).field("password", &"***").finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The account exists.
    Done(String),
    /// Nothing was done (no manager is connected to this node): it stays requested.
    Pending(String),
    Failed(String),
}

pub trait Provisioner: Send + Sync {
    fn provision(&self, req: &ProvisionRequest) -> Vec<(String, Outcome)>;
}

/// The default: no manager connected. Everything stays "requested" until the node is connected to its services.
pub struct NoopProvisioner;

impl Provisioner for NoopProvisioner {
    fn provision(&self, req: &ProvisionRequest) -> Vec<(String, Outcome)> {
        req.services.iter().map(|s| (s.clone(), Outcome::Pending("in attesa: questo nodo non è ancora collegato ai servizi".into()))).collect()
    }
}

// ---------------------------------------------------------------- running commands
pub trait Runner: Send + Sync {
    /// Runs a command and returns its output, or an error text.
    fn run(&self, argv: &[String]) -> Result<String, String>;
}

impl<T: Runner + ?Sized> Runner for std::sync::Arc<T> {
    fn run(&self, argv: &[String]) -> Result<String, String> {
        (**self).run(argv)
    }
}

/// Runs the commands for real, with a time limit.
pub struct ShellRunner {
    pub timeout_secs: u64,
}

impl Runner for ShellRunner {
    fn run(&self, argv: &[String]) -> Result<String, String> {
        let (prog, args) = argv.split_first().ok_or("comando vuoto")?;
        let mut child = std::process::Command::new(prog)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("impossibile eseguire {prog}: {e}"))?;
        let start = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let out = child.wait_with_output().map_err(|e| e.to_string())?;
                    let text = String::from_utf8_lossy(if status.success() { &out.stdout } else { &out.stderr }).trim().to_string();
                    return if status.success() { Ok(text) } else { Err(if text.is_empty() { format!("{prog} è terminato con {status}") } else { text }) };
                }
                Ok(None) if start.elapsed().as_secs() >= self.timeout_secs => {
                    let _ = child.kill();
                    return Err(format!("{prog}: tempo scaduto dopo {} secondi", self.timeout_secs));
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

/// Does not run anything: it records the commands (to see what a node *would* do, and for tests).
#[derive(Default)]
pub struct RecordingRunner {
    pub commands: Mutex<Vec<Vec<String>>>,
    /// Commands whose text contains one of these fail.
    pub fail_on: Mutex<Vec<String>>,
}

impl Runner for RecordingRunner {
    fn run(&self, argv: &[String]) -> Result<String, String> {
        self.commands.lock().unwrap_or_else(|e| e.into_inner()).push(argv.to_vec());
        let line = argv.join(" ");
        if self.fail_on.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|f| line.contains(f.as_str())) {
            return Err(format!("errore simulato: {line}"));
        }
        Ok("(prova: non eseguito)".into())
    }
}

// ---------------------------------------------------------------- the real manager: Docker Compose services
pub struct ComposeProvisioner<R: Runner> {
    pub runner: R,
    /// Container names (see deploy/compose.yaml).
    pub dms: String,
    pub synapse: String,
    pub wpcli: String,
}

impl<R: Runner> ComposeProvisioner<R> {
    pub fn new(runner: R) -> Self {
        let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        ComposeProvisioner { runner, dms: env("OPENPOLIS_DMS", "openpolis-mail"), synapse: env("OPENPOLIS_SYNAPSE", "openpolis-matrix"), wpcli: env("OPENPOLIS_WPCLI", "openpolis-wpcli") }
    }

    fn sh(&self, args: &[&str], secret: &str) -> Result<String, String> {
        let argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        // an error message must never carry the password back to the page or to a log
        self.runner.run(&argv).map_err(|e| if secret.is_empty() { e } else { e.replace(secret, "***") })
    }

    fn mail(&self, r: &ProvisionRequest) -> Result<String, String> {
        let addr = format!("{}@{}", r.username, r.domain);
        self.sh(&["docker", "exec", &self.dms, "setup", "email", "add", &addr, &r.password], &r.password)?;
        self.sh(&["docker", "exec", &self.dms, "setup", "quota", "set", &addr, r.mail_quota], &r.password)?;
        Ok(format!("indirizzo {addr}, spazio {}", r.mail_quota))
    }

    fn chat(&self, r: &ProvisionRequest) -> Result<String, String> {
        self.sh(
            &["docker", "exec", &self.synapse, "register_new_matrix_user", "-c", "/data/homeserver.yaml", "-u", &r.username, "-p", &r.password, "--no-admin", "http://localhost:8008"],
            &r.password,
        )?;
        Ok(format!("@{}:{}", r.username, r.domain))
    }

    fn insta(&self, r: &ProvisionRequest) -> Result<String, String> {
        let (site, addr) = (format!("{}.{}", r.username, r.domain), format!("{}@{}", r.username, r.domain));
        self.sh(&["docker", "exec", &self.wpcli, "wp", "user", "create", &r.username, &addr, &format!("--user_pass={}", r.password), "--role=subscriber", "--porcelain"], &r.password)?;
        self.sh(&["docker", "exec", &self.wpcli, "wp", "site", "create", &format!("--slug={}", r.username), &format!("--title={}", r.username), &format!("--email={addr}"), "--porcelain"], &r.password)?;
        self.sh(&["docker", "exec", &self.wpcli, "wp", "option", "update", "blog_upload_space", &r.blog_quota_mb.to_string(), &format!("--url={site}")], &r.password)?;
        Ok(format!("blog su https://{site}, spazio {} MB", r.blog_quota_mb))
    }
}

impl<R: Runner> Provisioner for ComposeProvisioner<R> {
    fn provision(&self, r: &ProvisionRequest) -> Vec<(String, Outcome)> {
        r.services
            .iter()
            .map(|s| {
                let res = match s.as_str() {
                    "subdomain" => Ok(format!("https://{}.{}", r.username, r.domain)), // served by the proxy, nothing to create
                    "mail" => self.mail(r),
                    "chat" => self.chat(r),
                    "insta" => self.insta(r),
                    other => Err(format!("servizio sconosciuto: {other}")),
                };
                (s.clone(), match res { Ok(d) => Outcome::Done(d), Err(e) => Outcome::Failed(e) })
            })
            .collect()
    }
}

/// Where a member finds a service.
pub fn link(service: &str, username: &str, domain: &str) -> Option<String> {
    match service {
        "subdomain" | "insta" => Some(format!("https://{username}.{domain}")),
        "mail" => Some(format!("https://mail.{domain}")),
        "chat" => Some(format!("https://chat.{domain}")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<String> {
        CATALOG.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn choices_are_checked() {
        assert_eq!(validate(&["chat".into(), "mail".into(), "mail".into()], &all()).unwrap(), vec!["mail", "chat"]);
        assert!(validate(&["insta".into()], &all()).is_err(), "the blog needs the subdomain");
        assert!(validate(&["insta".into(), "subdomain".into()], &all()).is_ok());
        assert!(validate(&["boh".into()], &all()).is_err());
        assert!(validate(&["mail".into()], &["chat".into()]).is_err(), "not offered by this node");
        assert!(validate(&[], &all()).unwrap().is_empty(), "no services is allowed: a member can have only the wallet and the vote");
    }

    #[test]
    fn the_password_does_not_leak_through_debug_or_errors() {
        let r = ProvisionRequest { username: "fulano".into(), domain: "openpolis.site".into(), password: "segretissima".into(), services: vec!["mail".into()], mail_quota: "1G", blog_quota_mb: 3072 };
        assert!(!format!("{r:?}").contains("segretissima"));
        let rec = RecordingRunner::default();
        rec.fail_on.lock().unwrap().push("add".into());
        let p = ComposeProvisioner::new(rec);
        let out = p.provision(&r);
        match &out[0].1 {
            Outcome::Failed(e) => assert!(!e.contains("segretissima"), "{e}"),
            other => panic!("{other:?}"),
        }
    }
}
