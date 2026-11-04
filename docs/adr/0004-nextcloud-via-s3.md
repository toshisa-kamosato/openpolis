# ADR-0004 — Nextcloud come livello file/chat, collegato alla rete tramite S3

**Stato:** SUPERATA da ADR-0009: Nextcloud e l'archiviazione di file sono stati tolti dal progetto. · ~~accettata~~

**Decisione.** Nell'istanza personale, file e chat sono Nextcloud (File e Talk). Nextcloud usa come *archiviazione primaria a oggetti* un endpoint **S3-compatibile** esposto da `luxd` (crate futuro `lux-s3`), che scrive nella rete distribuita con cifratura lato client ed erasure coding.

**Perché.** Nextcloud supporta già l'archiviazione a oggetti S3: non serve modificarlo. Il database e le sessioni di Nextcloud restano sul disco locale dell'istanza.

**Prototipo.** Le istanze girano in container OCI (Podman/Docker) sul nodo ospite; le microVM (Firecracker/Cloud Hypervisor) arrivano in seguito. WordPress (con il connettore già sviluppato) e Nextcloud sono due container della stessa istanza.

**Avvertenza.** Talk non è cifrato end-to-end per i messaggi di testo: se serve E2EE vera, valutare Matrix come servizio affiancato (ADR da scrivere).
