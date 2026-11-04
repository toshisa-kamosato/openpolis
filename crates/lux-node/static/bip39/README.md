# BIP-39 word lists

`italian.txt`, `english.txt`, `spanish.txt` are the official lists of [BIP-39](https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki)
(Marek Palatinus, Pavol Rusnak, Aaron Voisine, Sean Bowe), licence **MIT**, copied unchanged from
`https://github.com/bitcoin/bips/tree/master/bip-0039`. 2048 words each.

openpolis uses them only to *write down* a 128-bit secret as 12 words (11 bits per word, 4 checksum bits).
The key is derived from the 128 bits, not from the words, so the same secret can be shown in any of the lists.
`static/wordlists.js` is generated from these files by `jstests/make_wordlists.py`.
