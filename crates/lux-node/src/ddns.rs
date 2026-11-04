// SPDX-License-Identifier: GPL-2.0-or-later
//! Dynamic DNS built into the node (ADR-0015): when the public IP address changes, a URL given by the DNS provider is
//! called. Any provider that updates with an HTTP GET works (dyndns2-compatible services, DuckDNS, Namecheap...):
//! the URL is a template in which `{ip}` is replaced by the address.
use std::net::IpAddr;

pub trait Http: Send + Sync {
    fn get(&self, url: &str) -> Result<String, String>;
}

/// The host of a URL, for logs: the rest of it (user, password, token) is secret.
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    authority.rsplit('@').next().unwrap_or("").to_string()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// The address has not changed.
    Unchanged(IpAddr),
    /// The address changed and the provider was told.
    Updated(IpAddr),
}

/// One round: ask what our address is, and tell the provider if it changed. `last` is the address the provider has.
pub fn update_once(http: &dyn Http, ip_url: &str, update_template: &str, last: &mut Option<IpAddr>) -> Result<Step, String> {
    let text = http.get(ip_url).map_err(|e| format!("indirizzo pubblico non disponibile da {}: {e}", host_of(ip_url)))?;
    // only a real address goes into the URL: whatever the address service answers cannot inject anything
    let ip: IpAddr = text.trim().parse().map_err(|_| format!("{} non ha risposto con un indirizzo IP", host_of(ip_url)))?;
    if *last == Some(ip) {
        return Ok(Step::Unchanged(ip));
    }
    if !update_template.contains("{ip}") {
        return Err("l'indirizzo di aggiornamento deve contenere {ip}".into());
    }
    let url = update_template.replace("{ip}", &ip.to_string());
    http.get(&url).map_err(|e| format!("aggiornamento rifiutato da {}: {}", host_of(&url), e.replace(&url, "<url>")))?;
    *last = Some(ip);
    Ok(Step::Updated(ip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        ip: Mutex<String>,
        calls: Mutex<Vec<String>>,
        fail_update: bool,
    }
    impl Http for Fake {
        fn get(&self, url: &str) -> Result<String, String> {
            self.calls.lock().unwrap().push(url.to_string());
            if url.starts_with("https://ip.example") {
                Ok(self.ip.lock().unwrap().clone())
            } else if self.fail_update {
                Err(format!("403 su {url}"))
            } else {
                Ok("good".into())
            }
        }
    }
    const IPU: &str = "https://ip.example/";
    const UPD: &str = "https://user:segreto@dns.example/update?host=openpolis.site&ip={ip}";

    #[test]
    fn it_updates_only_when_the_address_changes() {
        let f = Fake { ip: Mutex::new("203.0.113.7\n".into()), calls: Mutex::new(vec![]), fail_update: false };
        let mut last = None;
        assert_eq!(update_once(&f, IPU, UPD, &mut last).unwrap(), Step::Updated("203.0.113.7".parse().unwrap()));
        assert!(f.calls.lock().unwrap().last().unwrap().ends_with("ip=203.0.113.7"));
        assert_eq!(update_once(&f, IPU, UPD, &mut last).unwrap(), Step::Unchanged("203.0.113.7".parse().unwrap()));
        assert_eq!(f.calls.lock().unwrap().len(), 3, "the second round only asked for the address");
        *f.ip.lock().unwrap() = "2001:db8::7".into();
        assert!(matches!(update_once(&f, IPU, UPD, &mut last).unwrap(), Step::Updated(_)));
    }

    #[test]
    fn whatever_the_address_service_says_cannot_inject_anything() {
        let f = Fake { ip: Mutex::new("1.2.3.4&evil=1".into()), calls: Mutex::new(vec![]), fail_update: false };
        let mut last = None;
        assert!(update_once(&f, IPU, UPD, &mut last).is_err());
        assert_eq!(f.calls.lock().unwrap().len(), 1, "the provider was not called");
        assert!(last.is_none());
    }

    #[test]
    fn a_refusal_is_retried_and_secrets_stay_out_of_the_messages() {
        let f = Fake { ip: Mutex::new("203.0.113.7".into()), calls: Mutex::new(vec![]), fail_update: true };
        let mut last = None;
        let e = update_once(&f, IPU, UPD, &mut last).unwrap_err();
        assert!(!e.contains("segreto"), "{e}");
        assert!(e.contains("dns.example"));
        assert!(last.is_none(), "not remembered: the next round tries again");
        assert!(update_once(&f, IPU, "https://dns.example/update", &mut last).is_err(), "a template without {{ip}} is refused");
        assert_eq!(host_of(UPD), "dns.example");
    }
}
