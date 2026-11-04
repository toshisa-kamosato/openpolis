# ADR-0005 — Crediti interni prima della catena ($LUX fase 0–1)

**Stato:** accettata · **Decisione di:** il committente.

**Decisione.** Nessuna blockchain per ora. `lux-ledger` (da creare in M7) tiene un registro locale (SQLite) di **crediti $LUX non trasferibili**: chi offre archiviazione e calcolo li guadagna, chi li usa li spende. Misure da `lux-meter` (cgroup, byte-ora), ricevute firmate dalle due parti quando i protocolli saranno stabili.

**Confini.** Nessuna vendita, scambio o conversione in valuta reale. Prima di rendere trasferibile il token (fase 3) servono revisione di sicurezza e parere legale (società in Spagna: MiCA, AML/KYC, normativa sui titoli).
