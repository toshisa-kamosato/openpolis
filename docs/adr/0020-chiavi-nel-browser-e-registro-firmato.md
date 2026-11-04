# ADR-0020 — Chiavi del portafoglio nel browser, frase di recupero e registro firmato

**Stato:** accettata (il committente ha scelto la frase di recupero di 12 parole il 2026-10-26) · precisa ADR-0013 · implementata in `lux-node` (registro, API) e `static/` (portafoglio nel browser).

**Obiettivo.** Il nodo non può spendere i LUX di un membro: ogni movimento è firmato nel browser dell'utente, il nodo verifica la firma, applica il motore (`lux-economy`) e scrive il movimento in un registro concatenato che chiunque può riverificare.

## Chiavi e frase di recupero
- **Frase:** 12 parole = 128 bit di entropia + 4 bit di controllo, scritte con le liste ufficiali **BIP-39** (italiano, inglese, spagnolo; licenza MIT, `static/bip39/`). Il controllo individua gli errori di battitura. Si accetta maiuscole, spazi in più e, per lo spagnolo, parole senza accenti.
- **Chiave:** **Ed25519** (WebCrypto), derivata dai 128 bit — non dalle parole — con HKDF-SHA256, sale `openpolis/v1/<id della rete>` e informazione `ed25519-signing-key`. Quindi: la stessa frase si può scrivere in italiano, inglese o spagnolo e dà la stessa chiave; la stessa frase dà chiavi diverse su reti diverse. Ho scelto Ed25519 al posto di P-256 perché nasce da un seme; richiede un browser recente (Chrome 137+, Firefox 129+, Safari 17+).
- **Uso quotidiano:** la chiave privata sta in IndexedDB come chiave **non estraibile**: nessuno script può rileggerla. Su un altro browser si reinserisce la frase.
- **Il nodo non vede mai la frase né la chiave privata.** Chi perde la frase perde i LUX: nessun recupero tramite l'amministratore (scartato perché non sarebbe sovrano).
- **Registrazione:** dopo il login, il browser invia la chiave pubblica con una prova di possesso (firma di `register-key:v1:<rete>:<utente>:<chiave>`) e la password; una sola volta per account.

## Movimento e registro
- **Pagamento:** JSON con ordine dei campi fisso `{v, net, kind, from, to, amount (centesimi), nonce, memo, t}`, firmato dal mittente. Il nodo controlla: sessione, rete, mittente = utente collegato, firma, `nonce` = ultimo + 1 (contro i replay), poi chiama `Economy::pay`.
- **Registro:** tabella SQLite concatenata con SHA-256 (ogni voce contiene l'hash della precedente). Voci: `genesis`, `register`, `key`, `pay`, `close_cycle` (e, pronte per dopo, `abandon` e `mint_vote`). Le voci di sistema le firma la chiave del nodo; i `pay` solo quella dell'utente.
- **Lo stato si ricostruisce rileggendo il registro**, controllando catena, hash, firme, prove di possesso, nonce e regole del motore; se qualcosa non torna il nodo **rifiuta di avviarsi** (`luxd verify` lo fa a richiesta).
- **Cicli:** `luxd run` chiude da solo i cicli di 28 giorni e li scrive nel registro (spese di rete e lavori votati sono vuoti finché non esiste il parlamento).

## Cosa protegge e cosa no (livello L0, un solo nodo)
- Protegge: il nodo **non può inventare un pagamento** a nome di un membro (provato da un test), né cambiare un importo, né togliere o riordinare voci senza che si veda.
- **Non protegge:** un amministratore malintenzionato può **censurare** (non registrare un pagamento), **fermare** il nodo, o rifare da capo l'intero registro con una storia diversa e firmarla con la chiave del nodo, finché altri nodi non conservano e confrontano il registro (livello L1, ADR-0013). Anche le voci di sistema (iscrizioni, cicli) sono credute sulla fiducia del nodo.
- Non è stato provato in browser veri; le prove usano browser simulato (jsdom), IndexedDB finta e WebCrypto di Node.
