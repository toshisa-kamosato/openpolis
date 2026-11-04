# ADR-0017 — La piattaforma web e le sue cinque sezioni

**Stato:** accettata come mappa; ordine di costruzione da decidere.

**Servizi per ogni membro:** posta cifrata, chat Matrix, galleria personale (il blog di insta_wp: privata o collegata al social), portafoglio, accesso al parlamento.

**Sezioni della piattaforma** (una per rete, accessibili con l'account della rete):
1. **Social** — il feed degli amici (cronologico, nessun algoritmo di raccomandazione: coerente con il manifesto), profili, amici, «mi piace», gruppi. Le foto vengono dalle gallerie collegate.
2. **Mercatino** — annunci divisi in categorie (decise dall'amministratore e poi dal parlamento); contatto via chat; nessun deposito a garanzia nella prima versione.
3. **Parlamento** — proposte, discussione, votazioni (ADR-0013); l'elettorato sono i membri della rete.
4. **Piazza** — annunci di eventi e concerti: data, luogo, categoria, calendario, esportazione iCal.
5. **Accademia** — offrire e ricevere corsi: descrizione, calendario, posti, modalità (in presenza o online via chat/videochiamata esterna), prezzo in $LUX o gratuito. Nessun hosting di file (ADR-0009).

**Dove vivono i dati.** Annunci, eventi e corsi sono pubblicati dall'autore (nella sua galleria o nel nodo) e la piattaforma li indicizza per categoria, data e zona; voti e portafoglio stanno nel registro della rete (ADR-0013).

**Moderazione e responsabilità.** Mercatino, eventi e corsi richiedono segnalazioni, regole sugli annunci vietati e un responsabile per rete: va deciso chi (amministratore o parlamento). Per vendite tra privati e attività professionali valgono norme di consumo e fiscali (Spagna/UE): consulente.

**Lo scheletro Flask `social-hub`** (account, amicizie, like, gruppi, collegamento di istanze con codice) copriva un hub *globale* con propri account. Con account a livello di rete va riscoperto come **modulo Social** della piattaforma: da ridisegnare prima di continuarlo.
