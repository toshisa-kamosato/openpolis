# ADR-0007 — Postura di sicurezza del prototipo

**Stato:** accettata · **Decisione di:** il committente («più rilassati con i protocolli di sicurezza» in questa fase).

**Decisione.** Per il prototipo (circa 10 utenti, 2–3 client, tutti persone note) si accettano scorciatoie elencate in `docs/security-debt.md`, ognuna con la condizione che obbliga a ripagarla. Il registro è il contratto: una scorciatoia non scritta lì non è ammessa.

**Resta obbligatorio anche ora:**
1. cifratura lato client dei dati archiviati;
2. nessun segreto nel repository;
3. nessuna crittografia o protocollo di autenticazione inventati;
4. password con argon2 e TLS su ogni servizio pubblico;
5. backup verificati dei dati degli utenti (le persone si fidano delle proprie foto).

**Prima di ammettere sconosciuti:** pagare tutto il debito di sicurezza e fare una revisione esterna (milestone M8).
