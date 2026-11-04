# ADR-0016 — Rete di nodi senza VPN: partecipare a una rete esistente o crearne una nuova; offgrid

**Stato:** accettata come direzione; dettagli da verificare nei prototipi.

**Una rete = una comunità.** Chi installa il primo nodo *crea una rete* (nome, dominio, valuta, amministratore). Un nuovo nodo può *partecipare a una rete esistente* con un invito (`luxd join`) oppure fondarne un'altra.

**Collegamento tra nodi senza VPN.** I nodi dialogano su Internet con TLS reciproco (certificati emessi dall'autorità della rete). Chi sta dietro NAT usa la **rete libp2p** (relay e hole punching): lo scheletro `lux-net`, che avevo messo in pausa, **torna utile** proprio per questo (oggi compila ma non è mai stato eseguito: prima prova in M8). Almeno un nodo per rete deve essere raggiungibile pubblicamente.

**Reti locali offgrid (LoRaWAN e simili).** Limiti fisici da progettare fin d'ora:
- LoRaWAN trasporta **pochi byte per messaggio** (in Europa da circa 51 a 222 byte di dati utili) e impone un **duty cycle** (circa 1% del tempo): adatto a messaggi brevi, schede di voto firmate, annunci e sincronizzazione di piccoli registri; **non** a posta, galleria o pagine web. Inoltre LoRaWAN è una rete a stella con gateway; per una maglia peer-to-peer si guarda a LoRa «puro» (stile Meshtastic).
- I servizi completi in una rete offgrid funzionano nella **rete locale** (Wi-Fi/Ethernet): il nodo serve il suo dominio locale con DNS interno e certificati dell'autorità della rete. I dati si sincronizzano con altre reti appena c'è un collegamento (store-and-forward).
