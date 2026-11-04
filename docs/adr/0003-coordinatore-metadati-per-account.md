# ADR-0003 — Coordinatore dei metadati: uno per account, sul nodo dell'utente

**Stato:** SUPERATA da ADR-0009: senza archiviazione distribuita non serve un coordinatore dei metadati. · ~~accettata (dettagli da definire in M3)~~

**Decisione.** L'indice di ciò che l'utente ha archiviato (oggetti, segmenti, nodi che li ospitano) vive in un *coordinatore* che gira sul nodo dell'utente. Nessun coordinatore centrale.

**Rischio e rimedio.** Se il nodo dell'utente è spento o perso, l'indice sparisce. Quindi: il **manifest** dell'account (indice cifrato, versionato, indirizzato per contenuto) viene replicato nella rete come qualunque altro oggetto, e il suo *puntatore di testa* è un record firmato dalla chiave dell'account (pubblicato nella DHT). Un nuovo nodo con la chiave dell'account ricostruisce tutto.

**Da progettare in M3:** formato del manifest, firma e versione del puntatore, risoluzione dei conflitti tra due nodi dello stesso account, frequenza di replica.
