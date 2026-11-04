# ADR-0010 — Creazione dell'account con PIN di 6 cifre

**Stato:** accettata.

**Decisione.** L'amministratore genera dalla propria pagina un **PIN di 6 cifre**; chi lo riceve apre la pagina di creazione, lo inserisce e sceglie nome utente, password e servizi (entro quelli concessi dal PIN).

**Perché regge.** Sei cifre sono solo 10^6 combinazioni: la sicurezza viene dai limiti attorno al PIN, non dal PIN:
- **monouso** e **a scadenza** (predefinita 24 ore, massimo 7 giorni);
- salvato come HMAC-SHA256 con segreto del nodo, mai in chiaro;
- massimo 5 tentativi falliti ogni 15 minuti per IP e 50 all'ora in tutto il nodo; dopo il limite la pagina risponde in modo uniforme;
- il PIN porta con sé le impostazioni scelte dall'amministratore (servizi, quote, eventuale nome utente riservato, ruolo);
- dopo l'uso si imposta una password robusta e si mostra *una sola volta* la frase di recupero del portafoglio.

**Da sapere.** Se davanti al nodo c'è un reverse proxy (per esempio quello dello stack dei servizi) il limite per IP deve leggere `X-Forwarded-For` solo da quel proxy. Un PIN intercettato prima dell'uso vale come invito: va consegnato a mano o su un canale cifrato.
