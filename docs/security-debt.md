# Registro del debito di sicurezza (prototipo)

Ogni riga è una scorciatoia consapevole, valida per il prototipo (circa 10 utenti, 2–3 client, persone note). **Da pagare prima di ammettere sconosciuti o reti con amministratori non fidati.** Una scorciatoia non scritta qui non è ammessa (ADR-0007).

| # | Scorciatoia | Perché ora va bene | Si paga quando |
|---|-------------|--------------------|----------------|
| 1 | Nessun modello di minacce formale né revisione esterna | Partecipanti noti | M11 |
| 2 | L'amministratore del nodo vede in chiaro galleria, posta al momento della consegna e chat dei ponti (ADR-0009) | L'amministratore è una persona della rete | Prima di reti con amministratori non fidati |
| 3 | Il nodo è esposto direttamente a Internet (nessun filtro intermedio) | Pochi utenti; firewall e aggiornamenti automatici | Prima dell'apertura pubblica |
| 4 | Invito con PIN di 6 cifre con limiti di tentativi (ADR-0010) | Pochi inviti, consegnati a mano | Prima dell'apertura pubblica |
| 5 | Ponti Telegram/WhatsApp: testo in chiaro nel ponte e rischio di blocco dell'account | Solo su richiesta, con consenso | Prima di proporli a persone non tecniche |
| 6 | Posta: reputazione dell'IP del nodo o del relay; DKIM/SPF/DMARC manuali nel registrar | Pochi messaggi | Prima di usarla per comunicazioni importanti |
| 7 | Registro L0: un solo nodo firma i blocchi; nessuna revisione del portafoglio | La valuta non ha valore economico | Prima che $LUX valga qualcosa (M5/M11 + parere legale) |
| 8 | Token API del DNS (DDNS e certificato wildcard) conservato sul nodo | Una sola persona accede | Hardening: permessi minimi del token, cifratura a riposo |
| 9 | Installer e aggiornamenti senza firma verificata dall'utente | 2–3 client | Prima della distribuzione pubblica |
| 10 | Antispam solo per account (senza commissioni, ADR-0018): reti aperte non protette | Rete su invito | Prima di reti aperte |
| 11 | Backup dei servizi non ancora provati con un ripristino reale | Dati di prova | **Prima di mettere dati veri** |
| 12 | Aggiornamento del DNS dinamico: breve indisponibilità al cambio di IP, e il rilevamento dell'IP usa un servizio esterno | Accettabile | Rilevamento con STUN o nodi della rete |

**Non è debito, è regola:** chiavi del portafoglio e della cifratura della posta solo nelle mani dell'utente; nessun segreto nel repository; nessuna crittografia inventata (voto segreto con schema collaudato); argon2 e TLS; backup verificati.
