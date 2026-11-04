# ADR-0015 — Nome, dominio e DNS: openpolis.site, DNS dinamico incorporato, niente VPN né proxy

**Stato:** accettata (ottobre 2026) · **Decisione di:** il committente. Sostituisce ADR-0006 e ADR-0011.

**Decisione.**
1. Il progetto si chiama **openpolis**; il dominio di esempio e di configurazione predefinita è **`openpolis.site`** (il dominio vero lo sceglie chi installa).
2. Il **dominio** si imposta durante l'installazione del *primo nodo*, insieme all'account dell'amministratore, e si usa **direttamente**: nessun proxy intermedio. L'installer stampa i record da aggiungere nel registrar (A/AAAA per `@` e per `*`, MX, SPF, DKIM, DMARC e, se serve, il record per la sfida dei certificati) e aspetta che siano visibili.
3. Il client incorpora un **client di DNS dinamico** che aggiorna gli indirizzi quando cambia l'IP pubblico. Protocolli previsti: *dyndns2* (generico, lo parlano molti servizi), API di Cloudflare, DuckDNS; altri su richiesta.
4. **Niente VPN** (WireGuard escluso). Niente VPS-proxy obbligatoria: una VPS può essere semplicemente uno dei nodi.

**Conseguenze reali da conoscere (non sono scelte, sono vincoli del mondo):**
- **Raggiungibilità:** il nodo deve ricevere connessioni dall'esterno (porte 80/443 e per la posta 25, 587/465, 993). Dietro un router servono l'inoltro delle porte o UPnP; con CGNAT (comune nelle linee mobili e in alcuni provider) **non è possibile** senza un intermediario. L'installer deve verificarlo e dirlo chiaramente.
- **Posta da una linea domestica:** molti provider bloccano la porta 25 e gli IP residenziali sono nelle liste di spam; senza DNS inverso (PTR) la posta in uscita spesso viene rifiutata. Soluzione prevista: **modalità di invio configurabile**: diretto (se l'IP lo permette) oppure tramite un *relay SMTP* esterno scelto dall'amministratore.
- **Certificati:** con record `*` serve un certificato *wildcard*, cioè la sfida DNS-01, quindi un token API del registrar; senza, un certificato per sottodominio con limiti di emissione. L'installer chiede quale strada.
- **Dominio unico per rete, nomi non globali:** ogni rete ha il proprio dominio; non esiste più un registro centrale dei nomi.
- L'IP dinamico si aggiorna con ritardo (TTL): brevi interruzioni al cambio di IP.
