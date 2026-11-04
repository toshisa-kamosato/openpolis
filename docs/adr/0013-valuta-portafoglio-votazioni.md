# ADR-0013 — Valuta, portafoglio e votazioni (a fasi)

**Stato:** accettata; conferma l'ADR-0005 (crediti prima della catena) e lo estende.

**Portafoglio.** Non custodial, nel browser: le chiavi si generano e restano lì (WebCrypto), cifrate con la passphrase dell'utente; frase di recupero mostrata una sola volta; il nodo vede solo transazioni firmate.

**Registro («blockchain») per comunità, a livelli.**
- **L0 — un nodo:** registro append-only con blocchi concatenati da hash e firmati dal nodo, in SQLite. Basta per un piccolo gruppo.
- **L1 — più nodi della stessa comunità:** replica con consenso Raft (crate `openraft`): tollera il guasto di nodi, ma si fida di tutti i nodi della comunità.
- **L2 — tra comunità o nodi non fidati:** consenso bizantino (valutare CometBFT con applicazione in Rust, o Substrate). Si decide solo se serve davvero.
Parametri impostati sul primo nodo dall'amministratore: nome della comunità, nome e simbolo della valuta (predefinito $LUX), regole di emissione (da decidere: a offerta fissa, coniata dall'amministratore, o guadagnata con un'attività).

**Votazioni.**
- *Voto aperto* (assemblee, deleghe): schede firmate e pubbliche sul registro, conteggio verificabile da chiunque. Fattibile subito.
- *Voto segreto*: richiede crittografia specifica (firme cieche, prove a conoscenza zero o mixnet). **Non si inventa**: si adotta uno schema collaudato. Da studiare: Helios, Belenios, Vocdoni; piattaforme di partecipazione: Decidim, CONSUL, Loomio.
- Diritto di voto: un membro = un voto, legato all'account creato con PIN.
- Per votazioni con valore legale (associazioni, enti) serve parere di un consulente.
