# ADR-0014 — Nodo, installer e cluster

**Stato:** AGGIORNATA da ADR-0015 e ADR-0016: l'installer chiede dominio e DNS dinamico (non più le credenziali del proxy); il cluster non usa WireGuard.

**Installer.** Uno script `install.sh` per Linux (Debian/Ubuntu per primi): controlla requisiti e porte; installa Docker (con Compose) e WireGuard; crea l'utente `lux`; scarica `luxd` verificando checksum/firma; avvia la procedura guidata (nome della comunità, nome e simbolo della valuta, dominio, token di iscrizione al gateway, credenziali del primo amministratore); genera i segreti; scrive e avvia lo stack Docker Compose; stampa l'indirizzo della pagina di amministrazione. Deve essere **ripetibile** (rilanciarlo non rompe nulla) e permettere di **modificare** la configurazione in seguito (`luxd config`, `luxd upgrade`, `luxd backup`/`restore`).

**`luxd`.** L'agente del nodo, in Rust: stato in SQLite, API e pagina di amministrazione (axum), emissione e uso dei PIN, provisioning dei servizi tramite adattatori, registro (ADR-0013), controllo di salute, backup. Il lavoro pesante lo fanno i container dei servizi.

**Cluster.** Il primo nodo è il *nodo fondatore*. Altri nodi si aggiungono con `luxd join <token>`; la connettività passa da WireGuard (all'inizio con la VPS come hub). I servizi di un utente stanno su un nodo «di casa», con configurazione e backup replicati; il registro si replica con Raft. Una comunità con un solo nodo non esegue nulla di tutto questo.

**La rete libp2p.** Lo scheletro `lux-net` (identità, scoperta dei peer, gossip, relay) **compila ma non è stato eseguito**; per una comunità con pochi nodi WireGuard + Raft sono più semplici. Resta in pausa; potrà tornare per scoperta e federazione *tra comunità*.
