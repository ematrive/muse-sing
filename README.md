# Muse Sing

App per macOS (Tauri 2 + React) che trasforma una partitura **MuseScore 4** (`.mscz`) in un progetto **OpenUtau** (`.ustx`). In OpenUtau le voci cantano davvero le parole scritte sotto le note, con un voicebank DiffSinger.

Documentazione per chi usa l'app: [docs/MANUALE.md](docs/MANUALE.md), che si legge anche dentro l'app nella scheda «Manuale». Le novità di ogni versione sono in [CHANGELOG.md](CHANGELOG.md).

## Cosa fa

- Legge dal `.mscz` le note e le sillabe di ogni voce. Le parti senza testo (organo, pianoforte) vengono saltate.
- **Srotola ritornelli, volte, D.C./D.S. al Fine/al Coda** come la riproduzione di MuseScore. Alla seconda passata usa la seconda strofa.
- Scrive le sillabe come vuole il phonemizer DiffSinger: la parola intera sulla prima nota, `+` sulle sillabe successive e `+~` sui melismi.
- **Italiano** (phonemizer *DIFFS IT*). Scrive esplicitamente la pronuncia quando la conversione di OpenUtau sbaglia con questi voicebank:
  - "i" semiconsonante: *pietà* non diventa "gietà";
  - doppia "r" vibrante: *terra* non diventa "tera";
  - "i" accentata in iato: *Dio*, *mio*.
- **Inglese** (phonemizer *DIFFS EN+*), con eccezioni per le forme arcaiche (*ask'd*, *feel'st*, *couldst*…).
- Esclude le note piccole di richiamo (non suonate) e le acciaccature.
- Avvisa di ciò che non gestisce:
  - strofe in più rispetto alle passate;
  - testo nella seconda voce del rigo;
  - cambi di metro;
  - battute "simile".
- Trova i cantanti installati in OpenUtau (`~/Library/OpenUtau/Singers`) e ricorda la scelta per ogni lingua.

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
| Italiano | Gahata Meiji v160 (LUNAI Project) | https://github.com/lunaiproject/lunai_singers/releases/download/Gahata_Meiji_v160/Gahata_Meiji_v160.zip |
| Inglese | TIGER DS v106 (tigermeat) | https://github.com/spicytigermeat/tiger_diffsinger/releases/download/v106/TIGER_DS_v106_PACK.zip (lo zip dentro `Voice Library/`) |

Installazione manuale, la più affidabile:

```bash
mkdir -p ~/Library/OpenUtau/Singers/<Nome> && cd ~/Library/OpenUtau/Singers/<Nome> && unzip ~/Downloads/<voicebank>.zip
```

## Installazione e rilasci

Il `.dmg` per Mac (Apple Silicon e Intel) è nelle [release](https://github.com/ematrive/muse-sing/releases): si crea da solo su GitHub quando si pubblica un tag `vX.Y.Z` (`.github/workflows/release.yml`). Ad ogni push su `main` girano build e test (`.github/workflows/ci.yml`). L'app installata trova da sola le versioni nuove all'avvio. Come pubblicare una versione e la chiave che firma gli aggiornamenti: [docs/rilascio.md](docs/rilascio.md).

## Creare l'app sul Mac

Una volta sola:

```bash
xcode-select --install
brew install node pnpm rustup
rustup-init -y
```

Poi, dalla cartella `app/`:

```bash
pnpm release:mac        # crea release/MuseSing-<versione>-macOS.dmg (Apple Silicon + Intel)
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

- `src-tauri/src/score.rs`: lettura del `.mscz`, ordine di esecuzione (ritornelli, salti).
- `src-tauri/src/lyrics.rs`: sillabe → parole, pulizia, regole italiane, eccezioni inglesi.
- `src-tauri/src/ustx.rs`: scrittura del progetto OpenUtau (`.ustx` 0.10).
- `src-tauri/src/corpus.rs`: frasi da registrare (esercizi, vocalizzi, brani) e controllo della copertura di fonemi e note.
- `src-tauri/src/audio.rs`: guida sintetizzata, registrazione dal microfono (cpal), WAV (hound).
- `src-tauri/src/voice.rs`: cartella del cantante (profilo, frasi, registrazioni).
- `src-tauri/src/singers.rs`: cantanti installati in OpenUtau (l'id è la cartella di `character.txt`, relativa a `Singers`).
- `src-tauri/tests/fixtures.rs`: confronta i progetti con quelli dello script Python di riferimento (`../tools/mscz_to_ustx.py`) sui quattro brani di prova. Le partiture del coro non sono nel repository: senza `src-tauri/tests/fixtures/` questi test vengono saltati.
- `preview.html`: interfaccia nel browser con il backend simulato, solo per lo sviluppo (`pnpm dev`, poi `/preview.html?auto=1`). La scheda di conversione usa le analisi dei brani (`src/preview-santo.json`, `src/preview-sonnet.json`), anch'esse solo in locale.

## Licenza

Codice: [MIT](LICENSE), © 2026 Emanuele Trivella. La licenza non riguarda i voicebank e il vocoder (con licenze proprie, non commerciali) né le voci registrate, che restano di chi le ha registrate.
