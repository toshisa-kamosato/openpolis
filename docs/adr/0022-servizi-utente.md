# ADR-0022 — Servizi per utente: posta, chat, blog, sottodominio

**Stato:** il piano di controllo è accettato e provato; **l'installazione dei servizi veri (cartella `deploy/`) è una bozza mai avviata**. · precisa ADR-0009 e ADR-0012; usa ADR-0015.

## Che cosa c'è
- **Catalogo:** `subdomain` (nome.dominio), `mail` (nome@dominio, 1 GB), `chat` (Matrix), `insta` (blog e galleria, 3 GB). Portafoglio e voto nel parlamento ci sono sempre. Il blog richiede il sottodominio.
- **Due livelli di scelta:** l'amministratore decide quali servizi offre il nodo (`luxd init --services …`); ogni membro sceglie tra questi **all'iscrizione** (caselle nella pagina `/join`) e può aggiungerne o riprovarne altri da `/services`.
- **Stato per servizio e membro** (`requested`, `active`, `failed`, con un dettaglio leggibile) in SQLite. Un servizio che fallisce non ferma gli altri.
- **La password non si conserva mai.** Il nodo la riceve all'iscrizione (o quando il membro la riscrive in `/services`), la dà al *gestore* che crea gli account, e la dimentica: un test verifica che non compaia nei file del database; i messaggi d'errore la oscurano. Conseguenza: aggiungere o riprovare un servizio richiede di riscrivere la password.
- **Gestore (`Provisioner`):** di serie *nessuno* (tutto resta «in attesa»); con `luxd run --provision compose` esegue `docker exec` nei container di `deploy/compose.yaml`:
  - posta: `setup email add nome@dominio …` e `setup quota set … 1G` (docker-mailserver);
  - chat: `register_new_matrix_user … --no-admin` (Synapse);
  - blog: `wp user create`, `wp site create` e `wp option update blog_upload_space 3072` (un solo **WordPress Multisite** con un sito per membro; la quota di 3 GB è quella nativa di WordPress e conta i file caricati, non il database).
- **Sottodomini e certificati:** un solo Caddy con carattere jolly e certificati *su richiesta*; prima di chiedere un certificato Caddy domanda a `luxd` (`/api/tls-ask`), che dice sì solo per il dominio, `mail.`, `chat.`, `matrix.` (se offerti) e per i sottodomini dei membri che li hanno scelti. Nessuno può far chiedere certificati per nomi a caso. Limite di Let's Encrypt: circa 50 certificati a settimana per dominio.
- **Matrix:** `luxd` serve i file `/.well-known/matrix/server` e `/client` così gli identificativi sono `@nome:dominio` e il server sta su `matrix.dominio`.
- **DNS:** `luxd dns --ip …` stampa i record da aggiungere (A/AAAA per `@` e `*`, MX, SPF, DMARC, DKIM) e ricorda il DNS inverso; `luxd ddns` tiene il DNS allineato quando cambia l'IP (richiesta GET con `{ip}` nell'indirizzo del fornitore; provato contro un server finto).

## Scostamenti dalle richieste e limiti (da sapere)
- Il blog sta nel **sottodominio del membro** (`nome.dominio`), non in `/insta`; `/mail` e `/chat` rimandano al webmail e a Element.
- **Non ci sono ancora** i ponti Telegram e WhatsApp (ogni ponte ha una configurazione sua; WhatsApp è non ufficiale e rischia il blocco dell'account), la **cifratura della posta a riposo** (c'è il TLS in transito) e l'**aspetto «Instagram»** del blog (i modelli di `insta_wp.py` vanno ancora applicati ai siti dei membri).
- **Chi amministra il nodo vede in chiaro** la password al momento dell'iscrizione, la posta alla consegna, il blog, e le chat che passano dai ponti (ADR-0009). Le chat Matrix tra membri sono cifrate da un capo all'altro (la cifratura è attiva per impostazione predefinita nelle stanze).
- Il gestore passa la password come argomento del comando (visibile per un istante con `ps` agli utenti della macchina) e `luxd` ha bisogno del socket di Docker, che equivale a essere root: gira sull'host con un utente proprio. Su un nodo condiviso va ripensato.
- Posta dal computer di casa: porta 25 spesso bloccata, indirizzi domestici in lista di spam, niente DNS inverso: serve una VPS o un relay.

## Che cosa è provato e che cosa no
Provato con test automatici: scelta e controllo dei servizi, comandi inviati al gestore (con un esecutore che li registra), stato per servizio, password assente dai file, errori senza password, elenco dei nomi con certificato, pagine di iscrizione e dei servizi, DNS dinamico (con server finto), sintassi di `install.sh`, `compose.yaml` e della configurazione di Element. **Non provato:** qualunque contenitore Docker, il server di posta, Synapse, WordPress, Caddy, l'emissione dei certificati, i record DNS veri.
