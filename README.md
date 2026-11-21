# Muse Sing

App per macOS (Tauri 2 + React) che fa cantare una partitura **MuseScore 4** (`.mscz`) o **MusicXML** (`.musicxml`, `.mxl`): le voci cantano davvero le parole scritte sotto le note, con un voicebank DiffSinger. L'audio si genera nell'app (motore DiffSinger in Rust, derivato da OpenUtau) e si esporta in MP3, FLAC o WAV. Ogni brano è un progetto `.msng` (zip con `project.json` e una copia della partitura; i vecchi `.musesing` si aprono ancora). Lingue: italiano, inglese e latino ecclesiastico (trascritto a regole nei fonemi italiani, `latin.rs`).

Documentazione per chi usa l'app: [docs/MANUALE.md](docs/MANUALE.md), che si legge anche dentro l'app nella scheda «Manuale». Le novità di ogni versione sono in [CHANGELOG.md](CHANGELOG.md).

## Cosa fa

- Legge dal `.mscz` le note e le sillabe di ogni voce. Le parti senza testo (organo, pianoforte) diventano l'accompagnamento, suonato da un organo additivo (`accomp.rs`).
- **Srotola ritornelli, volte, D.C./D.S. al Fine/al Coda** come la riproduzione di MuseScore. Alla seconda passata usa la seconda strofa.
- Scrive le sillabe come le vuole il fonemizzatore DiffSinger (convenzioni di OpenUtau): la parola intera sulla prima nota, `+` sulle sillabe successive e `+~` sui melismi.
- **Italiano** (G2P italiano di OpenUtau). Scrive esplicitamente la pronuncia dove il G2P sbaglia:
  - "i" semiconsonante: *pietà* non diventa "gietà";
  - doppia "r" vibrante: *terra* non diventa "tera";
  - "i" accentata in iato: *Dio*, *mio*.
- **Inglese** (G2P ARPAbet+ di OpenUtau), con eccezioni per le forme arcaiche (*ask'd*, *feel'st*, *couldst*…).
- Esclude le note piccole di richiamo (non suonate) e le acciaccature.
- Avvisa di ciò che non gestisce:
  - strofe in più rispetto alle passate;
  - testo nella seconda voce del rigo;
  - cambi di metro;
  - battute "simile".
- Trova i cantanti nella cartella propria di Muse Sing (`~/Library/Application Support/app.musesing.desktop/Singers`) e, se c'è, in quella di OpenUtau, e ricorda il cantante di ogni voce.
- **Genera l'audio** con un cantante per voce, su CPU, con cache per voce; velocità ridotta per lo studio; mix con volume/pan/muta/solo, loop di un tratto, rullo con tutte le parti; export MP3/FLAC/WAV e basi di studio (un MP3 per voce con quella voce in evidenza).
- **Progetti `.msng`** (`project.rs`): salva/apri, progetti recenti, «ricarica partitura» che conserva le scelte delle voci rimaste.
- **Ritocchi** per voce (`edits.rs`, `src/views/project/Editor.tsx`): fonemi e tempi dei fonemi per parola (o per tutto il brano), curve di intonazione, dinamica, timbro e velocità delle consonanti in tick della partitura; legati all'id stabile delle note (`battuta.passata.offset`), rigenerano solo le frasi toccate.

## Registra un cantante

La scheda **«Registra un cantante»** serve a creare un voicebank DiffSinger italiano con la voce di un corista. È la fase 1 del piano: dopo vengono l'etichettatura automatica e l'addestramento su GPU.

1. **Nuovo cantante:** nome, tipo di voce ed estensione comoda. Muse Sing crea una cartella con:
   - `profile.json`;
   - `corpus.json`, con le frasi da cantare;
   - `takes.json`;
   - `wavs/`.
2. **Frasi da cantare,** circa 5–12 s l'una, con note e parole già scritte:
   - circa 100 esercizi che coprono tutti i suoni dell'italiano, le doppie e i dittonghi, in tre registri (medio, grave, acuto);
   - vocalizzi su tutta l'estensione;
   - i brani del coro, aggiunti con «Aggiungi un brano del coro» (`.mscz` più la voce da cantare). Vengono tagliati in frasi alle pause.
3. **Registrare una frase:** quattro colpi di conteggio, poi si canta con la melodia guida in cuffia, che si può disattivare. La registrazione si ferma da sola.
   - Il WAV è mono a 24 bit e inizia 0,6 s prima della prima nota.
   - Avvisa se il segnale satura o è troppo basso.
4. **Scorciatoie:**
   - spazio: registra;
   - G: guida;
   - P: riascolta;
   - ← →: frase precedente o successiva;
   - Invio: prossima frase da registrare.

Durata: circa 37 min di esercizi, più i brani. Per una prova bastano il registro medio e i vocalizzi, circa 13 min.

Il primo avvio chiede il permesso per il microfono (`Info.plist` + `Entitlements.plist`). Se lo hai negato: Impostazioni di Sistema → Privacy e sicurezza → Microfono.

## Voicebank consigliati (uso non commerciale, citando l'autore)

| Lingua | Voicebank | Download |
|---|---|---|
| Italiano e inglese | TIGER DS v106 (tigermeat) | https://github.com/spicytigermeat/tiger_diffsinger/releases/download/v106/TIGER_DS_v106_PACK.zip (lo zip dentro `Voice Library/`) |

Tiger non si può ridistribuire: il pulsante «Scarica Tiger» (`tiger.rs`) lo scarica dalla release ufficiale ed estrae lo zip interno in `Singers/Tiger`. Installazione manuale:

```bash
S=~/Library/Application\ Support/app.musesing.desktop/Singers; mkdir -p "$S/<Nome>" && cd "$S/<Nome>" && unzip ~/Downloads/<voicebank>.zip
```

## Installazione e rilasci

Il `.dmg` per Mac con Apple Silicon (dalla 0.5.0: ONNX Runtime non ha binari per Mac Intel) è nelle [release](https://github.com/ematrive/muse-sing/releases): si crea da solo su GitHub quando si pubblica un tag `vX.Y.Z` (`.github/workflows/release.yml`). Ad ogni push su `main` girano build e test (`.github/workflows/ci.yml`). L'app installata trova da sola le versioni nuove all'avvio. Come pubblicare una versione e la chiave che firma gli aggiornamenti: [docs/rilascio.md](docs/rilascio.md).

## Creare l'app sul Mac

Una volta sola:

```bash
xcode-select --install
brew install node pnpm rustup
rustup-init -y
```

Poi, dalla cartella `app/`:

```bash
pnpm release:mac        # crea release/MuseSing-<versione>-macOS.dmg (Apple Silicon)
```

Apri il `.dmg` e trascina **Muse Sing** in Applicazioni. Senza un certificato Apple l'app ha solo la firma ad-hoc. Al primo avvio macOS la dichiara "danneggiata", e si sblocca una volta con:

```bash
xattr -cr "/Applications/Muse Sing.app"
```

## Sviluppo

```bash
pnpm install
pnpm tauri dev               # app in modalità sviluppo
cd src-tauri && cargo test   # test del motore (su Linux serve libasound2-dev per cpal)
```

- `src-tauri/src/score.rs`: lettura del `.mscz` e riconoscimento del formato (`read_score`), ordine di esecuzione (ritornelli, salti), id stabile di ogni nota; `musicxml.rs`: lettura di MusicXML (`.musicxml`/`.mxl`) con la stessa disposizione (`assemble`).
- `src-tauri/src/tiger.rs`: download di TIGER DS dalla release ufficiale e installazione del voicebank (lo zip dentro il pacchetto) nella cartella Singers.
- `src-tauri/src/lyrics.rs`: sillabe → parole, pulizia, regole italiane, eccezioni inglesi, riconoscimento della lingua (per parole intere); `latin.rs`: latino ecclesiastico nei simboli G2p italiani, una vocale per sillaba.
- `src-tauri/src/ustx.rs`: disposizione delle note e delle sillabe per il motore (formato dei progetti OpenUtau `.ustx` 0.10, usato anche dai test di confronto; l'app non esporta più `.ustx`).
- `src-tauri/src/corpus.rs`: frasi da registrare (esercizi, vocalizzi, brani) e controllo della copertura di fonemi e note.
- `src-tauri/src/audio.rs`: guida sintetizzata, registrazione dal microfono (cpal), WAV (hound).
- `src-tauri/src/voice.rs`: cartella del cantante (profilo, frasi, registrazioni).
- `src-tauri/src/singers.rs`: cantanti installati in OpenUtau (l'id è la cartella di `character.txt`, relativa a `Singers`) e nella cartella di Muse Sing (prefisso `musesing:`), con la compatibilità con il motore.
- `src-tauri/src/ds/`: motore DiffSinger, port in Rust del codice di OpenUtau (`OpenUtau.Core/DiffSinger`, MIT) con ONNX Runtime (`ort`, linkato staticamente): G2P (`g2p.rs`, modelli italiano e ARPAbet+ di OpenUtau in `resources/g2p/`), fonemizzazione e durate (`phonemize.rs`), pitch, modello acustico e vocoder (`render.rs`).
- `src-tauri/src/sing.rs`: render di tutte le voci (e dell'accompagnamento) in un thread, con annullamento, velocità, ritocchi e cache su disco per voce e per frase (l'intonazione prevista dal modello è in cache a parte, così le curve si sommano sempre alla stessa); `mix.rs`: mix stereo con solo ed export (hound, flacenc, LAME); `project.rs`: file `.msng`; `accomp.rs`: organo per le parti strumentali.
- `src-tauri/examples/ds_render.rs`: render da riga di comando (`cargo run --release --example ds_render -- <brano.mscz> <voicebank> <out.wav>`); `tests/sing.rs` fa un render completo se `MUSESING_TEST_VOICEBANK` punta a un voicebank.
- `src-tauri/tests/fixtures.rs`: confronta i progetti con quelli dello script Python di riferimento (`../tools/mscz_to_ustx.py`) sui quattro brani di prova. Le partiture del coro non sono nel repository: senza `src-tauri/tests/fixtures/` questi test vengono saltati.
- `preview.html`: interfaccia nel browser con il backend simulato, solo per lo sviluppo (`pnpm dev`, poi `/preview.html?auto=1`). La scheda di conversione usa le analisi dei brani (`src/preview-santo.json`, `src/preview-sonnet.json`), anch'esse solo in locale.

## Licenza

Codice: [MIT](LICENSE), © 2026 Emanuele Trivella. Il motore DiffSinger deriva da OpenUtau (MIT) e l'export MP3 usa LAME (LGPL): vedi [NOTICE](NOTICE). La licenza non riguarda i voicebank e il vocoder (con licenze proprie, non commerciali) né le voci registrate, che restano di chi le ha registrate.
