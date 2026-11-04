# Installare un nodo openpolis su un computer vero

> **Stato: bozza mai provata.** `luxd` (il cuore: rete, registro, portafoglio, parlamento, scelta dei servizi) è collaudato
> con test automatici; **tutto ciò che sta in questa cartella no**: non ho potuto avviare Docker né i servizi nel mio ambiente.
> I file sono scritti dalla documentazione di ogni immagine. Aspettati di dover correggere dei dettagli: i punti più
> probabili sono elencati in fondo. Tienili sotto mano e dimmi cosa non funziona.

## Prima di cominciare
- Una macchina **Debian/Ubuntu** (VPS o computer di casa) con accesso come amministratore, 2 GB di RAM o più.
- Un **dominio** tuo, con accesso ai record DNS dal registrar.
- Le porte **80, 443** (web) e **25, 465, 587, 993** (posta) raggiungibili da Internet. Da casa: inoltro delle porte sul router;
  con CGNAT (alcune linee mobili e provider) non si può. Molti provider **bloccano la porta 25**: se è il tuo caso, la posta in
  arrivo non funzionerà (o chiedi lo sblocco, o usa una VPS).
- **Posta in uscita da una linea di casa**: gli indirizzi domestici sono nelle liste di spam e non hanno il DNS inverso: la
  posta verrà rifiutata da molti. Per provare va bene; per usarla davvero serve una VPS o un relay SMTP.

## Passi
1. Copia il repository sulla macchina e lancia `sudo deploy/install.sh`. Risponde alle domande (dominio, comunità, moneta,
   amministratore), compila `luxd` (la prima volta ci vogliono diversi minuti), avvia i servizi e il blog.
2. Alla fine stampa i **record DNS** da aggiungere (anche con `luxd dns --ip <IP>`). Aggiungili, aspetta qualche minuto.
3. Se il tuo IP cambia: `luxd ddns --update-url '<indirizzo del tuo fornitore con {ip}>'` (vedi sotto).
4. Apri `https://IL-TUO-DOMINIO/admin`, accedi, genera un PIN; con il PIN `https://…/join` crea l'account scegliendo i servizi.
   Poi `/wallet` (frase di 12 parole), `/parliament`, `/services`.

## DNS dinamico
`luxd ddns` chiama l'indirizzo di aggiornamento del tuo fornitore ogni volta che cambia l'IP pubblico. Va bene ogni servizio che
si aggiorna con una richiesta GET: DuckDNS (`https://www.duckdns.org/update?domains=NOME&token=TOKEN&ip={ip}`), Namecheap
(`https://dynamicdns.park-your-domain.com/update?host=@&domain=DOMINIO&password=PASSWORD&ip={ip}`), servizi compatibili con
dyndns2. Metti l'indirizzo in `OPENPOLIS_DDNS_URL` per non lasciarlo nella cronologia della shell (contiene un segreto).
Non copre Cloudflare, che vuole una richiesta con intestazioni: da aggiungere.

## Cosa NON c'è ancora
- **Ponti Telegram e WhatsApp**: non configurati (ogni ponte `mautrix-*` vuole una configurazione sua; WhatsApp è non ufficiale e
  rischia il blocco dell'account). Chi amministra il nodo vedrebbe in chiaro i messaggi che passano dai ponti.
- **Cifratura della posta a riposo**: non configurata. C'è il TLS in transito; le caselle sul disco non sono cifrate.
- **Aspetto "Instagram" del blog**: il blog è un WordPress Multisite normale (un sito per membro, 3 GB ciascuno); i modelli e gli
  script del blog di prova (`insta_wp.py`) vanno ancora applicati ai siti dei membri.
- Il blog vive nel **sottodominio del membro** (nome.dominio), non in `/insta`; `/mail` e `/chat` rimandano al webmail e a Element.

## Cose da controllare per prime (le più probabili da correggere)
1. **Certificati nel server di posta:** il percorso dei certificati di Caddy montato nel container di posta
   (`SSL_CERT_PATH` in `compose.yaml`) va verificato; se non esiste, la posta parte senza TLS valido.
2. **Synapse:** dopo `generate`, controlla in `synapse/homeserver.yaml` che il listener ascolti su un indirizzo raggiungibile
   dalla porta pubblicata (`bind_addresses`), e che `register_new_matrix_user` trovi il segreto di registrazione.
3. **WordPress Multisite** dietro Caddy: l'URL del sito principale è il dominio della piattaforma (servito da `luxd`), non da
   WordPress; verifica che `fulano.dominio` mostri il blog di Fulano.
4. **Roundcube** raggiunge il server di posta usando il nome pubblico `mail.dominio`: dentro la stessa macchina può servire
   un'impostazione "hairpin" o `extra_hosts`.
5. **Limiti di Let's Encrypt**: circa 50 certificati a settimana per dominio registrato. Per ~10 membri va bene.
6. **Il gestore dei servizi** (`luxd run --provision compose`) usa `docker exec` e passa la password del membro come argomento
   del comando (per un istante è visibile con `ps` agli utenti della macchina). Su una macchina con un solo amministratore è
   accettabile; per un nodo condiviso va cambiato.
7. L'utente `openpolis` fa parte del gruppo `docker`: su quella macchina equivale a essere root. Non darlo ad altri.
