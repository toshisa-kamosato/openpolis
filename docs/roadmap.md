# Roadmap (v0.3, dopo ADR-0015…0018)

Prototipo: ~10 utenti, 2–3 client. In grassetto ciò che si può verificare *senza* Docker (cioè nel mio ambiente di prova); il resto richiede una VM Linux con Docker.

| Milestone | Contenuto | Accettazione |
|-----------|-----------|--------------|
| **M1 `luxd` base** | ✅ **Fatto e provato** (`lux-node` + `luxd`): stato in SQLite; creazione della rete (nome, dominio, moneta) e amministratore; PIN a 6 cifre (HMAC, uso singolo, scadenza, limiti di tentativi); iscrizione con PIN; pagine `/admin` e `/join`. Mancano: collegare il motore economico (iscrizione → `register`), login dei membri, servizi per utente | 23 test (regole + HTTP) e prova reale con il programma |
| **M2 DNS dinamico** | ✅ `luxd dns` (record da aggiungere) e `luxd ddns` (aggiornamento via GET con `{ip}`, con Basic auth), provati con un server finto; **non provati con un fornitore vero**. Non copre Cloudflare | Prova reale su un dominio |
| M3 Servizi per utente | ✅ **Piano di controllo fatto e provato** (ADR-0022): scelta all'iscrizione, stato, password mai conservata, gestore `docker exec`, certificati su richiesta, pagina `/services`. ⚠️ **I servizi veri (posta, Matrix, WordPress, Caddy) sono in `deploy/`, bozza mai avviata**. Mancano ponti Telegram/WhatsApp, cifratura della posta a riposo, aspetto «Instagram» del blog | Su una macchina Linux con Docker: un membro creato ha posta, chat e blog funzionanti |
| M4 Installer | ⚠️ `deploy/install.sh` + guida (`deploy/README.md`): **bozza mai eseguita** (sintassi controllata) | Installazione su Ubuntu pulita; rilancio senza danni |
| **M5 Economia e registro** | ✅ Motore economico `lux-economy` (ADR-0019). ✅ Login dei membri. ✅ **Portafoglio nel browser con frase di 12 parole, registro firmato che applica il motore, pagina della banca nel portafoglio, chiusura automatica dei cicli** (ADR-0020). Mancano: parlamento (voti, `mint_vote`, lavori, spese), abbandoni dall'amministrazione, rotazione della chiave, prova in browser veri | Test di regola, di firma, di manomissione, di percorso completo e della pagina |
| **M6 Parlamento** | ✅ **Fatto e provato** (ADR-0021): proposte (testo, emissione, spesa, lavoro), voto aperto con quorum e maggioranza, chiusura automatica, effetti nel ciclo successivo (lavori riportati sui cicli seguenti), abbandoni dall'amministrazione, pagina `/parliament`. Mancano: proposte per cambiare i parametri, limiti alle iscrizioni decise dall'amministratore, voto segreto, delega, commenti | 16 test del parlamento + prova della pagina |
| **M7 Piattaforma: Social e Piazza** | **Account di rete (SSO), feed degli amici, profili, gruppi; eventi con calendario e iCal** (dal vecchio scheletro Flask, ridisegnato) | **Test + prova end-to-end** |
| **M8 Piattaforma: Mercatino e Accademia** | **Annunci per categoria; corsi con iscrizioni; segnalazioni** | **Test automatici** |
| M9 Più nodi | `luxd join`, TLS reciproco, **libp2p** per i nodi dietro NAT (prima prova della rete oggi non eseguita), registro replicato con Raft | Spegnendo un nodo la rete continua |
| M10 Offgrid | Rete locale con DNS interno e sincronizzazione differita; messaggi brevi e schede di voto su LoRa | Voto firmato consegnato tra due nodi senza Internet |
| M11 Sicurezza | Modello di minacce, revisione esterna, voto segreto con schema collaudato, pagamento del debito | Revisione senza problemi gravi aperti |

Idee future: federazione tra reti, consenso bizantino (L2), nodi di sconosciuti.
