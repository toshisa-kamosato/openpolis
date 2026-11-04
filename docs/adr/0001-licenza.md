# ADR-0001 — Licenza: GPL-2.0-or-later

**Stato:** accettata (ottobre 2026) · **Decisione di:** il committente ha scelto «GNU v2».

**Decisione.** Tutto il codice del progetto (client `luxd`/`lux`, hub, connettore WordPress) è sotto **GPL-2.0-or-later**, con intestazione `SPDX-License-Identifier: GPL-2.0-or-later` in ogni file.

**Perché «or-later» e non «only».** Molte librerie Rust sono sotto Apache-2.0 (per esempio `prost`, che useremo per i protocolli binari). La FSF considera Apache-2.0 incompatibile con GPLv2 *soltanto*, ma compatibile con GPLv3. Con «or-later» l'opera combinata si può distribuire sotto GPLv3 e il problema non esiste. WordPress stesso è GPLv2-or-later, quindi il connettore e gli script restano coerenti.

**Conseguenze.**
- Nextcloud (AGPL-3.0) e Matrix/altri servizi sono programmi separati che girano accanto a `luxd`: non vengono collegati al codice, quindi nessun problema di licenza.
- `deny.toml` elenca le licenze ammesse; `cargo deny check licenses` va eseguito in CI.
- Se in futuro si volesse **GPL-2.0-only**, bisognerebbe eliminare o sostituire ogni dipendenza solo-Apache: va deciso prima di ricevere contributi esterni.
- La GPL non protegge un servizio web usato solo via rete (il hub): se interessa quella protezione, valutare AGPL-3.0 per il solo hub.
- Questa non è consulenza legale.
