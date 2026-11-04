# ADR-0019 — Economia di openpolis: zecca, cassa comune e banca

**Stato:** accettata (ottobre 2026; interpretazioni confermate il 26/10) · **Decisione di:** il committente. Sostituisce ADR-0018 (niente commissioni) e precisa ADR-0005 e ADR-0013. Implementata in `crates/lux-economy`.

## Le regole (come le ha date il committente)
- **Zecca:** emette 1.000.000 LUX alla creazione del primo client. Può essere richiamata in circostanze speciali (anticipo straordinario alla banca) o con la maggioranza di una votazione. Possono proporre la votazione **tutti gli utenti registrati**, non solo l'amministratore.
- **Cassa comune** (una sola, decisione del committente): riceve 5.000 LUX alla creazione e 1.000 per ogni iscritto; finanzia i fidi; incassa le commissioni e la tassa sui saldi.
- **Banca:** il software che gestisce l'economia; i suoi dati compaiono nella pagina del portafoglio.
- **Account:** partono con 0 LUX. Il **ciclo personale** di 28 giorni parte dalla prima transazione dell'utente. **Fido:** 100 LUX alla prima transazione, 300 dopo 3 cicli solventi.
- **Pagamenti:** commissione 5% (2,5% venditore + 2,5% compratore) alla cassa comune. Il compratore paga prezzo + 2,5%; il fido si controlla su prezzo × 1,025.
- **Uso della cassa comune, in quest'ordine:** (1) spese di gestione della rete, (2) lavori sociali votati, (3) restituzione del resto agli utenti in proporzione alle commissioni pagate.
- **Debiti:** da coprire entro 28 giorni dalla transazione che li ha creati, il più vecchio per primo. Si copre con LUX propri o comprandoli in euro 1:1 da chi ha saldo o vende il proprio fido (**accordo tra privati fuori dal sistema**, il registro non tocca mai gli euro). Un debito scaduto blocca il conto fino alla solvenza.
- **Ciclo della rete:** 28 giorni dalla creazione del client.
- **Banca, a ogni ciclo:** deve tornare ai fidi non erogati (fidi finanziati meno debiti); il *surplus* è la differenza. Surplus negativo: si sospendono lavori e restituzioni; se non basta, anticipo straordinario dalla zecca, da restituire con i surplus dei cicli successivi, al netto delle spese di rete, prima di lavori e restituzioni.
- **Tassa sui saldi:** 5% sull'eccedenza oltre 300 LUX, alla cassa comune. **Abbandoni:** la banca cancella i debiti scoperti al ciclo successivo, a carico del surplus.
- **Unità:** due decimali (centesimi di LUX).

## Come è modellato
- `surplus = cassa − Σ max(0, fido − debito)` (i fidi non erogati). Un prestito sposta LUX dalla cassa al conto e crea un debito: il surplus non cambia. Le commissioni e la tassa lo aumentano; spese, lavori e restituzioni lo diminuiscono.
- **Chiusura di un ciclo, in questo ordine:** (1) si aggiornano i cicli personali; (2) tassa sui saldi; (3) abbandoni: il portafoglio copre i debiti, il resto si cancella, il conto si chiude; (4) spese di rete (sempre pagate); (5) se la cassa non copre i fidi non erogati: anticipo dalla zecca (la zecca emette la differenza se non ha abbastanza LUX) e si sospende tutto il resto; (6) altrimenti il surplus restituisce prima l'anticipo alla zecca; (7) solo a anticipo estinto: lavori votati, poi restituzioni proporzionali alle commissioni del ciclo (il resto dell'arrotondamento resta in cassa).
- **Invarianti verificate dopo ogni operazione:** `emesso = zecca + cassa + Σ portafogli` (nessuna moneta nasce o sparisce se non per emissione registrata), libri coerenti, e un'operazione fallita non lascia tracce.
- Tutti i numeri sono **parametri** (`Params`): in futuro si cambiano solo con un voto.

## Interpretazioni (confermate dal committente il 2026-10-26)
1. «Richiamare la zecca» = emettere altri LUX (anticipo straordinario o voto), non ritirarli. ✔
2. **Ogni movimento di LUX tra utenti paga la commissione del 5%** (2,5% + 2,5%), che sia un pagamento o un trasferimento: non esistono trasferimenti gratuiti. Un trasferimento può usare il fido come un pagamento. Restano senza commissione i flussi con la banca: prestiti, spese di rete, lavori votati, restituzioni, tassa sui saldi. ✔
3. «3 cicli solventi» = 3 di fila; un ciclo con un debito scaduto azzera il conteggio e riporta il fido a 100. ✔
4. **Abbandono:** il portafoglio copre prima i debiti, il resto si cancella e il conto si chiude; il debito cancellato è **trattenuto dal surplus**: se il surplus del ciclo non basta, si trattiene anche quello dei cicli successivi finché la perdita è coperta; il ciclo dopo che la trattenuta è completa, la somma torna distribuibile. ✔ (modello: `loss_target` e `loss_held`)
5. La tassa sui saldi si applica a ogni ciclo della rete, sul saldo di quel momento.
6. **Il debito si riduce solo con i LUX in entrata da altri utenti** (una vendita o un trasferimento, già tassati), mai con un pagamento diretto alla banca: chi è in debito usa prima i LUX che riceve per coprire i debiti, il più vecchio per primo, e solo il resto aumenta il saldo. Per questo saldo positivo e debito non coesistono. «Vendere il proprio fido» = pagare oltre il proprio saldo (fino al fido). Anche i pagamenti della banca (spese di rete, lavori, restituzioni) coprono prima i debiti. ✔
7. Un conto bloccato può ricevere e rimborsare, non spendere né prelevare.
8. La cassa riceve i 1.000 LUX alla **registrazione**, non alla prima transazione. Il fido si concede anche a chi inizia come venditore.

## Cose che il motore mostra e che vanno decise
- **Il surplus restituisce quasi tutto il capitale iniziale al primo ciclo**: con i valori predefiniti, la cassa (5.000 + 1.000 per iscritto) è molto più grande dei fidi da coprire (100–300 per utente), quindi la differenza viene distribuita come «resto» in proporzione alle commissioni pagate. Va bene? Chi commercia di più riceve di più; chi non commercia riceve zero.
- **La zecca basta per circa 995 iscritti** (1.000 LUX a testa su 1.000.000): oltre, serve una nuova emissione per voto.
- **Copertura dei fidi:** la cassa copre, per ogni membro, il fido che ha *adesso* (100 per i primi tre cicli di 28 giorni, poi 300), meno i suoi debiti. Quando un gruppo di membri passa a 300 serve altra copertura: se la cassa ha già restituito il resto, il surplus può diventare negativo e la banca chiede un anticipo alla zecca, da restituire con i surplus successivi (la regola originale). Il committente ha chiarito che il fido è 100 per i primi tre mesi e poi 300; l'ipotesi di tenere la copertura a 300 per tutti è stata scartata.
- **Frode con conti fittizi:** fido, pagamento a un complice, vendita in euro fuori dal sistema, abbandono. Perdita massima per conto: il fido (100, poi 300). Si limita con inviti a PIN e con le 3 verifiche di solvenza prima di salire a 300.
- **Aspetti legali (Spagna/UE):** commissioni, prestiti (fidi), tassa sui saldi e scambio LUX/euro tra privati toccano regole su servizi di pagamento, credito, moneta elettronica e cripto-attività (MiCA) e antiriciclaggio. Il motore non gestisce euro; prima di dare valore a $LUX serve un parere di un consulente.
