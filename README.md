# openpolis / luxd

**Al popolo quello che è del popolo.** Suite di software libero per l'autogoverno: democrazia, moneta, commercio, posta, chat e social come beni comuni digitali, su una rete distribuita di computer privati. Vedi [`docs/MANIFESTO.md`](docs/MANIFESTO.md). Una comunità installa **luxd** su uno o più nodi Linux, punta il proprio dominio ai nodi (DNS dinamico incorporato, senza VPN) e offre ai suoi membri sottodominio, posta cifrata, chat Matrix, blog, portafoglio $LUX e votazioni — tutto dal browser.

- Decisioni: [`docs/adr/`](docs/adr) (parti da ADR-0009)
- Piano: [`docs/roadmap.md`](docs/roadmap.md)
- Debito di sicurezza del prototipo: [`docs/security-debt.md`](docs/security-debt.md)
- Istruzioni per il modello di sviluppo: [`CLAUDE.md`](CLAUDE.md)

**Stato:** progettazione e prototipo. Lo scheletro Rust (`crates/`) compila; la rete libp2p è in pausa; il lavoro riprende da `luxd` base (M2).

Licenza: GPL-2.0-or-later (vedi `LICENSE` e ADR-0001).

## Provalo (prima fetta funzionante)

```bash
cargo build -p luxd
export LUX_ADMIN_PASSWORD='una frase lunga e strana'   # oppure lascia che luxd la chieda
luxd init --name "La mia comunità" --domain openpolis.site --admin-user clara
luxd run --listen 127.0.0.1:8080
# apri http://127.0.0.1:8080/admin, accedi e genera un PIN; chi lo riceve va su /join
luxd pin new --admin-user clara --ttl-hours 24    # alternativa da terminale
```

Dopo l'iscrizione: `/login`, poi `/wallet` per creare il portafoglio (12 parole da scrivere su carta: chi le perde perde i LUX), pagare e vedere i dati della banca. `/parliament` per proporre e votare (emissione di moneta, spese, lavori, decisioni): `luxd run` chiude da solo le votazioni scadute e i cicli. `luxd verify` ricontrolla tutto il registro.

Servizi (posta, chat, blog) e installazione su una macchina vera: `deploy/README.md` (**bozza mai provata**).

Test: `cargo test -p lux-economy -p lux-node`; test del browser: `cd crates/lux-node/jstests && npm install && LUXD_BIN=<percorso di luxd> npm test`. Il dominio, l'HTTPS e il DNS dinamico arrivano con M2 e M4.
