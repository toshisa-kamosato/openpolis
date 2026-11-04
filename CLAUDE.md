# CLAUDE.md — istruzioni per il modello che lavora su questo repository

Leggi per primi `docs/adr/` (soprattutto ADR-0009) e `docs/roadmap.md`. Lavora solo sulla milestone corrente.

## Il progetto in breve
**openpolis** — *Al popolo quello che è del popolo* (vedi `docs/MANIFESTO.md`) è una suite di software libero per l'autogoverno. Una comunità installa il client (`luxd`) su uno o più nodi Linux e crea una **rete** (o partecipa a una esistente). Il dominio (esempio: `openpolis.site`) punta direttamente ai nodi, con DNS dinamico incorporato; **niente VPN né proxy** (ADR-0015). Ogni membro, con un PIN di 6 cifre dato dall'amministratore (ADR-0010), ottiene posta cifrata (1 GB), chat Matrix (con ponti Telegram/WhatsApp), una galleria/blog (3 GB), un portafoglio per $LUX (commissione 5% e banca, ADR-0019) e il diritto di voto nel **parlamento**. La piattaforma ha cinque sezioni: social (feed degli amici), mercatino, parlamento, piazza, accademia (ADR-0017). Le reti possono essere anche locali e offgrid, con i limiti di LoRaWAN (ADR-0016).
Licenza: **GPL-2.0-or-later**. Società in Spagna. Prototipo: ~10 utenti, 2–3 client.

## Regole
1. **Onestà tecnica.** Distingui ciò che hai eseguito da ciò che supponi. Se un requisito non è realizzabile come scritto, dillo subito e proponi l'alternativa più vicina. Non dichiarare proprietà di sicurezza che il sistema non ha (vedi tabella in ADR-0009).
2. **Niente crittografia inventata.** Solo primitive e librerie consolidate; il voto segreto usa uno schema collaudato.
3. **Piccoli passi verificabili.** Test prima, esecuzione, esiti riportati. «Funziona» solo se l'hai eseguito.
4. **Decisioni tracciate** in `docs/adr/`; il debito di sicurezza in `docs/security-debt.md` (ogni scorciatoia ha una condizione di pagamento).
5. **Chiedi conferma** prima di scelte difficili da invertire (formati, nomi, licenza), spese, e tutto ciò che riguarda denaro, token o aspetti legali. Non dare consulenza legale.
6. **Rust:** edizione 2021+, `#![forbid(unsafe_code)]`, `clippy` e `rustfmt` puliti, niente `unwrap` fuori dai test, errori tipizzati. Documentazione in italiano; codice, commenti e commit in inglese.
7. **Ogni risposta:** cosa hai fatto · come l'hai verificato · cosa non hai potuto verificare · rischi e decisioni aperte · prossimo passo.

## Mappa del codice
- `crates/lux-core` — configurazione, percorsi, protocollo di controllo locale.
- `crates/luxd` — programma del nodo: `init [--services]`, `run [--provision noop|compose]` (chiude da solo cicli e votazioni), `pin new`, `status`, `verify`, `dns`, `ddns`.
- `deploy/` — **bozza mai avviata** dei servizi (Caddy, posta, Matrix, WordPress Multisite) e dell'installer. Non fidarti finché non è provata su una macchina vera (`deploy/README.md`).
- `crates/lux` — CLI.
- `crates/lux-node` — **il nodo:** SQLite, rete, amministratore, PIN, iscrizione, login, **registro firmato** (ADR-0020), portafoglio (`/wallet`), **parlamento** (`/parliament`, ADR-0021), **servizi utente** (`/services`, ADR-0022), DNS (`dns.rs`, `ddns.rs`), API web (tiny_http). `static/` = il portafoglio nel browser (chiave Ed25519 da frase di 12 parole BIP-39); `static/parliament.js` = la pagina del parlamento; `jstests/` = test del browser (`npm install && npm test`, serve `luxd` compilato).
- `crates/lux-economy` — **motore economico** (zecca, cassa comune, banca, fidi, cicli): logica pura, deterministica, con 24 test di regola e simulazione casuale; vedi ADR-0019.
- `crates/lux-net` — rete libp2p: compila, **mai eseguita**; prima prova in M8.
- Fuori da questo repo: `insta_wp.py`, `functions-snippet.php` (blog WordPress), `social-hub/` (scheletro Flask parziale, da ridisegnare come modulo Social: ADR-0017).

## Comandi
`cargo check --workspace` · `cargo test --workspace` · `cargo clippy --workspace -- -D warnings` · `cargo fmt --check` · `cargo deny check` (licenze, vedi `deny.toml`).

## Stato
M2 (`luxd` base: SQLite, comunità, PIN, amministrazione) è il prossimo lavoro. L'ambiente di prova non ha Docker: gli adattatori dei servizi si provano con versioni finte; per il resto serve una VM Linux.
