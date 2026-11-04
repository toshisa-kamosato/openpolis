> Aggiornato da ADR-0015…0018: nome openpolis, senza VPN né proxy, sezioni della piattaforma, $LUX senza commissioni.

# ADR-0009 — Nuovo perimetro: una suite di servizi per individui e comunità

**Stato:** accettata (ottobre 2026) · **Decisione di:** il committente («Nuove istruzioni»). Sostituisce ADR-0002, 0003 e 0004.

**Decisione.** Il progetto (nome del progetto: **openpolis**) è una **suite di software libero** che una comunità installa sui propri nodi Linux e che offre a ogni membro:
1. un sottodominio `fulano.openpolis.site`;
2. un indirizzo `fulano@openpolis.site` con 1 GB e pagina di amministrazione su `/mail`;
3. un account Matrix cifrato con ponti a Telegram e WhatsApp su `/chat`;
4. un blog per i media con 3 GB su `/insta` (il connettore già sviluppato);
5. un portafoglio per criptovalute sovrane, con $LUX come predefinita;
6. un software per votazioni collettive su blockchain;
tutto gestito dal browser. Un **nodo** (un solo server per un piccolo gruppo) o **più nodi collegati** (comunità grandi) forniscono la potenza di calcolo. `openpolis.site` e una VPS commerciale fanno solo da proxy.

**Cosa esce dal progetto:** Nextcloud, l'archiviazione distribuita di file (erasure coding, audit, riparazione), il calcolo su nodi di sconosciuti, il coordinatore dei metadati per account.

**Modello di fiducia.** Il nodo è gestito da qualcuno della comunità (l'amministratore). Chi amministra il nodo può in linea di principio leggere ciò che il nodo vede in chiaro. Per questo ogni servizio dichiara cosa protegge davvero:

| Servizio | Protetto da | L'amministratore del nodo può vedere |
|----------|-------------|--------------------------------------|
| Chat Matrix (tra utenti Matrix) | cifratura end-to-end | solo metadati |
| Chat tramite ponte Telegram/WhatsApp | nessuna: il ponte vede il testo | messaggi in chiaro |
| Email | TLS e cifratura a riposo con chiave dell'utente | il messaggio in arrivo (al momento della consegna), poi no |
| Blog | contenuto pensato per essere pubblico | tutto |
| Portafoglio | chiavi nel browser dell'utente | mai le chiavi, solo transazioni firmate |

**Conseguenze.** Il registro del debito di sicurezza si riscrive su questo modello; la crittografia diventa un requisito *per servizio*, non una proprietà della rete.
