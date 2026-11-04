# ADR-0012 — Servizi per utente: posta, chat, blog

**Stato:** accettata (candidati da validare con una prova su una VM pulita).

**Posta (1 GB, `/mail`).** Candidati: *Stalwart Mail Server* (scritto in Rust, IMAP/JMAP/SMTP, quote, più domini, cifratura a riposo con chiave OpenPGP o S/MIME dell'utente) oppure *Mailcow/Mailu* (Postfix+Dovecot in Docker). Invio: diretto oppure tramite relay SMTP esterno (ADR-0015). Interfaccia web: Roundcube/SnappyMail o quella del server scelto. «Criptata» significa: TLS in transito + cifratura a riposo con chiave dell'utente. La vera end-to-end tra due persone richiede OpenPGP (Autocrypt/WKD) e non vale per la posta da/verso altri provider.

**Chat (`/chat`).** Un homeserver Matrix per comunità (*Synapse* come predefinito perché i ponti sono maturi; le alternative più leggere si valutano dopo) più Element Web. Cifratura end-to-end predefinita. **Ponti** (mautrix-telegram e mautrix-whatsapp) attivabili dal singolo utente con una schermata di consenso che dica chiaramente: (1) il ponte vede i messaggi in chiaro e conserva la sessione dell'utente; (2) il collegamento a WhatsApp non è ufficiale e può portare a limitazioni dell'account.

**Galleria / blog (3 GB, `/insta`).** Privata o collegata alla sezione Social (ADR-0017). WordPress con il connettore già sviluppato, un container per utente (isolamento; con ~10 utenti il costo di memoria è accettabile). Va verificato il funzionamento in sottocartella (`/insta`): finora è stato provato alla radice. Quota: quota di progetto del filesystem (XFS/ext4) o controllo morbido via plugin; da decidere.

**Interfaccia di provisioning.** Per ogni servizio un adattatore con le operazioni `create`, `suspend`, `delete`, `set_quota`, `usage`. I test usano adattatori finti; quelli veri si provano sulla VM.
