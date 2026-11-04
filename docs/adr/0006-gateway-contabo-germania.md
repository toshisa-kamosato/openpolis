# ADR-0006 — Gateway su Contabo (Germania)

**Stato:** SUPERATA da ADR-0015: niente VPS proxy né VPN; il dominio punta direttamente ai nodi.

**Decisione.** Il punto di accesso al mondo reale per `openpolis.site` è una VPS commerciale **Contabo, datacenter in Germania** (UE). Esegue: DNS/proxy inverso per i sottodomini delle istanze, nodo di bootstrap e **relay** libp2p (`luxd --relay-server`), e il piano di controllo iniziale (formulario di creazione istanze).

**Prototipo.** Il proxy può terminare TLS (limite noto, vedi `docs/security-debt.md`); in seguito passthrough SNI con certificati nell'istanza. La VPS è un punto di centralizzazione dichiarato.

**Dati personali (GDPR).** La società/associazione è in Spagna e la VPS in UE: nessun trasferimento extra-UE previsto. Servono informativa, registro dei trattamenti e accordo con il fornitore (Contabo) — da preparare con un consulente.
