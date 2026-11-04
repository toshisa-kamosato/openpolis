// SPDX-License-Identifier: GPL-2.0-or-later
//! The DNS records to add at the registrar when a node is installed (ADR-0015): the domain is used directly,
//! with no proxy in between.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsRecord {
    pub kind: &'static str,
    /// Relative to the domain: "@" is the domain itself.
    pub name: String,
    pub value: String,
    pub note: &'static str,
}

/// `ipv4` and `ipv6` are the public addresses of the node. The DKIM key is created by the mail server, so it is a placeholder.
pub fn records(domain: &str, ipv4: Option<&str>, ipv6: Option<&str>) -> Vec<DnsRecord> {
    let r = |kind, name: &str, value: String, note| DnsRecord { kind, name: name.to_string(), value, note };
    let mut v = Vec::new();
    for (kind, ip) in [("A", ipv4), ("AAAA", ipv6)] {
        if let Some(ip) = ip {
            v.push(r(kind, "@", ip.into(), "il dominio"));
            v.push(r(kind, "*", ip.into(), "tutti i sottodomini (nome.dominio, mail, chat, matrix)"));
            v.push(r(kind, "mail", ip.into(), "il server di posta"));
        }
    }
    v.push(r("MX", "@", format!("10 mail.{domain}."), "dove arriva la posta"));
    v.push(r("TXT", "@", "\"v=spf1 mx -all\"".into(), "SPF: solo il tuo server può inviare posta per questo dominio"));
    v.push(r("TXT", "_dmarc", format!("\"v=DMARC1; p=quarantine; adkim=s; aspf=s; rua=mailto:postmaster@{domain}\""), "DMARC"));
    v.push(r("TXT", "mail._domainkey", "\"v=DKIM1; k=rsa; p=<CHIAVE>\"".into(), "DKIM: la chiave la crea il server di posta: `docker exec openpolis-mail setup config dkim domain DOMINIO`, poi copia il valore di mail.txt"));
    v.push(r("CAA", "@", "0 issue \"letsencrypt.org\"".into(), "facoltativo: solo Let's Encrypt può emettere certificati"));
    v
}

/// The reverse DNS (PTR) cannot be set at the registrar: it is set at the provider of the IP address.
pub const PTR_NOTE: &str = "Il DNS inverso (PTR) dell'IP deve puntare a mail.DOMINIO: si imposta presso chi ti dà l'IP (provider o VPS). Senza, molti server rifiutano la tua posta.";

pub fn table(domain: &str, recs: &[DnsRecord]) -> String {
    let mut out = format!("Record DNS da aggiungere per {domain}:\n\n");
    out.push_str(&format!("{:<6} {:<17} {}\n", "TIPO", "NOME", "VALORE"));
    for r in recs {
        out.push_str(&format!("{:<6} {:<17} {}\n         ({})\n", r.kind, r.name, r.value, r.note));
    }
    out.push_str(&format!("\n{PTR_NOTE}\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_records_for_a_domain() {
        let v = records("openpolis.site", Some("203.0.113.7"), Some("2001:db8::7"));
        let has = |k: &str, n: &str, val: &str| v.iter().any(|r| r.kind == k && r.name == n && r.value.contains(val));
        assert!(has("A", "@", "203.0.113.7") && has("A", "*", "203.0.113.7") && has("AAAA", "*", "2001:db8::7"));
        assert!(has("MX", "@", "10 mail.openpolis.site."));
        assert!(has("TXT", "@", "v=spf1 mx -all") && has("TXT", "_dmarc", "v=DMARC1") && has("TXT", "mail._domainkey", "DKIM1"));
        let only4 = records("openpolis.site", Some("203.0.113.7"), None);
        assert!(!only4.iter().any(|r| r.kind == "AAAA"));
        assert!(table("openpolis.site", &v).contains("PTR"));
    }
}
