# ADR-0008 — Protocollo di controllo locale: JSON su socket Unix (per ora)

**Stato:** accettata (provvisoria).

**Decisione.** `lux` parla con `luxd` tramite un oggetto JSON per riga su un socket Unix `~/.lux/luxd.sock` (permessi 0600). È semplice da provare e da debuggare.

**Poi.** Quando il vocabolario di comandi si stabilizza (dopo M3) si passa a gRPC (`tonic`/`prost`) con schema protobuf versionato. Il passaggio non tocca la rete tra nodi.
