# ADR-0011 — Gateway: VPS Contabo con EasyEngine 4 e Docker, solo proxy

**Stato:** SUPERATA da ADR-0015: niente proxy, niente WireGuard.

**Decisione.** La VPS pubblica ospita EasyEngine 4 (siti e proxy in container Docker, certificati Let's Encrypt) e fa **solo da proxy**: non conserva dati degli utenti. I nodi si collegano alla VPS con **WireGuard** (connessione in uscita dal nodo, quindi funziona anche dietro NAT domestico). Un piccolo servizio *registro* sulla VPS associa ogni sottodominio al nodo responsabile e genera la configurazione nginx.

**Punti da verificare prima di costruire** (non sono sicuro dei dettagli e dipendono dalla versione):
- se EasyEngine 4 permette di definire comodamente host che fanno proxy verso indirizzi WireGuard: altrimenti si usa un nginx/Caddy/Traefik in un container accanto, tenendo EasyEngine solo per i siti propri della VPS;
- **certificati:** per `*.openpolis.site` serve un certificato *wildcard* (sfida DNS-01, quindi API del provider DNS). Con un certificato per sottodominio i limiti di Let's Encrypt (circa 50 nuovi certificati a settimana per dominio registrato) diventano un collo di bottiglia oltre poche decine di utenti;
- **posta:** MX di `openpolis.site` sulla VPS che inoltra al nodo; l'invio esce dalla VPS con PTR, SPF, DKIM e DMARC. Controllare la politica di Contabo sulla porta 25 in uscita (spesso chiusa di default e da richiedere);
- il registro dei nomi è un punto di centralizzazione dichiarato: chi controlla `openpolis.site` controlla i nomi.

**Prototipo.** Il proxy termina TLS (debito di sicurezza n. 3).
