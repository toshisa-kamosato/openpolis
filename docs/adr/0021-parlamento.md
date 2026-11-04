# ADR-0021 — Il parlamento: proposte, voti ed effetti

**Stato:** accettata come primo disegno (le regole numeriche sono scelte provvisorie, da confermare) · precisa ADR-0013 e ADR-0019 · implementata in `lux-node` (`ledger.rs`, `node.rs`) e `static/parliament.js`.

## Regole
- **Chi propone:** ogni membro con il portafoglio. Al massimo **3 proposte aperte** per membro. Titolo fino a 120 caratteri, testo fino a 2000.
- **Tipi:** *testo* (consultazione, nessun effetto automatico); *emissione* (nuova moneta, fino alla quantità già emessa); *spesa di rete* (importo a un membro); *lavoro sociale* (importo a un membro, dal surplus); *abbandono* (dichiarare abbandonato un membro).
- **Durata:** da 1 a 28 giorni, scelta da chi propone.
- **Chi vota:** chi aveva già il portafoglio **quando la proposta è stata aperta** e ha ancora l'account (chi si iscrive o crea il portafoglio dopo non vota su quella proposta; chi è stato chiuso per abbandono nemmeno). Un voto per membro: sì, no o astenuto. **Voto aperto**: tutti vedono chi ha votato cosa. Si può cambiare finché la votazione è aperta; conta l'ultimo.
- **Esito:** serve che abbia votato almeno il **20%** degli aventi diritto (astenuti compresi), poi i «sì» devono essere **più dei «no»** (con un pareggio la proposta è respinta; gli astenuti non contano per la maggioranza). **Per un'emissione di moneta serve una maggioranza qualificata: almeno due terzi di sì tra sì e no** (scelta del committente). Esiti: approvata, respinta, senza quorum, annullata.
- **Chiusura:** la fa il nodo da solo appena scade il tempo, con una voce nel registro che dichiara il risultato; **il registro la accetta solo se coincide con i voti** e se la scadenza è passata.
- **Effetti:** *emissione* approvata → la zecca emette quei LUX nella stessa voce di chiusura. *Spesa* approvata → pagata per intero alla prossima chiusura di ciclo. *Lavoro* approvato → pagato dal surplus alla prossima chiusura di ciclo, nell'ordine di approvazione; se il surplus non basta, **il resto resta in coda e passa ai cicli successivi**.
- **Cosa il nodo non può fare** (provato da test): emettere moneta senza un voto (non esiste più la voce `mint_vote`), far pagare a un ciclo qualcosa che nessuno ha votato (la voce del ciclo deve elencare esattamente la coda approvata), dichiarare un esito diverso dai voti, chiudere in anticipo, inserire un voto dopo la scadenza, o votare al posto di un membro.
- **Abbandoni (decisi dal voto, scelta del committente):** si propone con il tipo *abbandono* contro un membro. **Salvaguardia mia da confermare:** si può proporre solo se quel membro **non ha firmato nulla (pagamenti, proposte, voti) da 84 giorni** (tre cicli), perché un abbandono fa tornare alla cassa comune i LUX rimasti nel suo conto e una maggioranza non deve poter confiscare i LUX di una persona attiva. Se il membro firma qualcosa mentre la votazione è aperta, la proposta approvata è **annullata**. Se approvata, l'account si ferma e al ciclo successivo la banca cancella i debiti scoperti (ADR-0019). L'amministratore non ha più alcun potere diretto sugli abbandoni.

## Limiti da conoscere
- **L'amministratore decide chi è elettore**: emette i PIN e quindi può iscrivere molte persone fittizie e pesare sui voti (anche un'emissione). Va limitato prima di usarlo con valore reale: per esempio iscrizioni che richiedono la garanzia di un membro o un voto, un tetto agli iscritti per ciclo, o la possibilità per il parlamento di rimuovere iscritti.
- **I parametri** (20%, maggioranza semplice, 3 proposte, 1–28 giorni) sono scritti nella voce iniziale del registro; cambiarli richiede una proposta di tipo «parametri», non ancora realizzata.
- Il quorum è lo stesso (20%) anche per l'emissione di moneta: se serve di più, è un parametro da alzare.
- Non esistono ancora: voto segreto (serve uno schema collaudato, ADR-0013), delega, proposte per cambiare i parametri o l'amministratore, revoca dei membri, discussione con commenti.
- Con un solo nodo (L0) valgono i limiti dell'ADR-0020: chi gestisce il nodo può censurare o fermare le votazioni.
