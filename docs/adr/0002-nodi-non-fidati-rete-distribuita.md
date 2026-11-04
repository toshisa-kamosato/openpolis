# ADR-0002 — Nodi non fidati e rete distribuita fin dall'inizio

**Stato:** SUPERATA da ADR-0009 (ottobre 2026): i nodi appartengono alla comunità che li gestisce e non sono più trattati come non fidati; non c'è più archiviazione distribuita. · ~~accettata~~

**Decisione.** I nodi sono trattati come **non fidati**: chi ospita dati o carichi può leggere la memoria e il disco della propria macchina. Non si promette riservatezza del *calcolo*. Si parte subito dalla rete peer-to-peer (libp2p) e dall'archiviazione distribuita.

**Conseguenze.**
- **Non negoziabile anche nel prototipo:** i dati utente si cifrano *lato client* prima di lasciare il nodo del proprietario (XChaCha20-Poly1305, chiave per oggetto). Costa poco e rende vera la frase «i nodi non vedono i tuoi file».
- Il calcolo (istanze WordPress/Nextcloud) su nodi altrui espone memoria e database a chi li ospita: va detto agli utenti. Per ora le istanze girano su nodi di persone note (circa 10 utenti, 2–3 client).
- Modalità B (TEE) e A (nodo di casa) restano possibili in futuro senza cambiare il resto.
