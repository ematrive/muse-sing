# Manuale di Muse Sing

Muse Sing fa cantare una partitura MuseScore: le voci cantano davvero le parole, non solo «oh oh». Serve soprattutto per **l'ascolto d'insieme**: sentire il brano cantato con il testo, voce per voce o tutte insieme, e portarsi a casa un MP3 per studiare.

L'app ha tre schede:

- **Canta una partitura:** genera l'audio cantato (un cantante per ogni voce) da ascoltare o esportare in MP3, FLAC o WAV. Se vuoi ritoccare la pronuncia in OpenUtau, crea anche il file `.ustx`.
- **Registra un cantante:** raccoglie le registrazioni per costruire un cantante italiano con la voce di un corista.
- **Manuale:** questa guida e le novità di ogni versione.

## Prima di iniziare

Servono tre cose, da installare una volta sola.

### 1. OpenUtau

Muse Sing canta da solo: OpenUtau serve solo se vuoi ritoccare un brano a mano (fonemi, curve) con il file `.ustx`, ed è il posto più comodo dove installare i cantanti.

1. Scarica il `.dmg` per macOS dall'ultima versione su https://github.com/stakira/OpenUtau/releases.
2. Trascina OpenUtau in Applicazioni.
3. Al primo avvio macOS dice che è «danneggiato». Apri il Terminale e scrivi:

   ```
   xattr -cr /Applications/OpenUtau.app
   ```

4. Apri OpenUtau una volta e chiudilo: così crea la sua cartella `~/Library/OpenUtau`.

### 2. I cantanti (voicebank)

Per ora Muse Sing usa **TIGER DS v106**, che canta sia in italiano sia in inglese: https://github.com/spicytigermeat/tiger_diffsinger/releases/download/v106/TIGER_DS_v106_PACK.zip

Nel pacchetto c'è un secondo zip dentro `Voice Library/`: è quello da decomprimere. Installalo a mano dal Terminale (il trascinamento dentro OpenUtau a volte chiede una codifica e poi non fa nulla):

```
mkdir -p ~/Library/OpenUtau/Singers/Tiger
cd ~/Library/OpenUtau/Singers/Tiger
unzip ~/Downloads/TIGER_DS_v106.zip
```

Riapri OpenUtau e controlla in **Strumenti → Cantanti** che i cantanti compaiano.

Muse Sing legge i cantanti dalla cartella di OpenUtau. Un cantante che vuoi usare solo in Muse Sing (per esempio quello costruito con la scheda Registra) va invece in `~/Library/Application Support/app.musesing.desktop/Singers`.

### 3. Muse Sing

Serve un Mac con processore Apple (M1 o successivi): sui Mac Intel funziona solo fino alla versione 0.4.0.

1. Apri il `.dmg` e trascina **Muse Sing** in Applicazioni.
2. Al primo avvio, se macOS dice che l'app è danneggiata, sbloccala una volta:

   ```
   xattr -cr "/Applications/Muse Sing.app"
   ```

**Aggiornamenti e novità.** All'avvio Muse Sing controlla se c'è una versione nuova. Se c'è, mostra le novità e chiede se aggiornare: con «Aggiorna» scarica, installa e si riavvia da sola. Il numero di versione in alto a destra apre le novità di ogni versione; un pallino viola segnala quelle che non hai ancora letto. Dopo un aggiornamento macOS può chiedere di nuovo il permesso per il microfono, una volta.

## Canta una partitura

1. Apri la scheda **Canta una partitura**.
2. **Trascina** il file `.mscz` nella finestra, oppure clicca e sceglilo. Compaiono titolo, metro, tempo, durata e l'elenco delle voci con le prime parole.
3. Controlla la **lingua**: viene riconosciuta da sola, ma puoi cambiarla.
4. Scegli l'**intonazione**:
   - **naturale:** il cantante decide come attaccare e legare le note, con un po' di vibrato. Suona più umano;
   - **esatta:** la voce sta ferma sulle note scritte. Utile per controllare l'intonazione della propria parte.
5. Spunta le **voci da cantare** e scegli il **cantante** di ognuna. Se il cantante ha più colori di voce (Tiger ne ha diversi), accanto compare la scelta del colore. Muse Sing ricorda il cantante di ogni voce (Soprano, Tenore…) da un brano all'altro.
6. Premi **Genera audio**. Il Mac calcola le voci una dopo l'altra: per un brano di 3-4 minuti con quattro voci servono alcuni minuti. Puoi annullare; la frase in corso finisce prima di fermarsi.
7. Premi **Ascolta**. Con la barra scegli da dove partire.
8. Regola il **mix** di ogni voce: volume, posizione a sinistra o a destra (pan), «muta» per escluderla. Le modifiche si sentono subito, senza rigenerare: per esempio alza la tua voce o muta le altre. Doppio clic su un cursore lo riporta a zero.
9. Premi **Esporta…** e scegli il formato: MP3 (per il telefono), FLAC (senza perdita di qualità) o WAV.

Le voci già generate con lo stesso cantante e la stessa partitura vengono riusate: se cambi il cantante di una sola voce, si rigenera solo quella. Se cambi voci, cantanti, lingua o intonazione dopo aver generato, Muse Sing chiede di generare di nuovo.

**Avvisi di pronuncia.** Sotto il lettore, «avvisi di pronuncia» elenca le parole che il cantante non sa dire del tutto (un suono che non ha). Di solito la parola si sente lo stesso, con quel suono saltato.

### Progetto OpenUtau (.ustx)

Per ritoccare fonemi e curve, premi **Crea .ustx** nel riquadro in basso e scegli dove salvarlo. Ogni voce usa il cantante scelto sopra.

### Ascoltare in OpenUtau

1. In OpenUtau, **File → Apri** e scegli il `.ustx`.
2. Premi Play. La prima volta OpenUtau calcola le voci e ci mette qualche secondo.
3. Ogni voce è una traccia: puoi silenziarla o ascoltarla da sola, per esempio per studiare la tua parte.
4. Per avere un file audio usa il menu di esportazione di OpenUtau (File → Esporta).

### Ritornelli e salti

Muse Sing segue la partitura come la riproduzione di MuseScore:

- **ritornelli:** alla seconda passata si canta la seconda strofa, se c'è;
- **finali 1. e 2.**;
- **D.C. e D.S. al Fine o al Coda**.

Un «Fine» senza D.C. o D.S. viene ignorato, come fa MuseScore. Se il coro canta un ultimo ritornello fino al Fine, aggiungi «D.C. al Fine» nella partitura.

### Come scrivere la partitura perché canti bene

- **Sillabe con il trattino** di MuseScore (*San-to*): Muse Sing ricompone le parole.
- **Melismi** (una sillaba su più note): usa la linea di estensione di MuseScore. Le note senza sillaba continuano la vocale.
- **Elisioni:** due sillabe sulla stessa nota si scrivono insieme sulla nota, come fate già (*to-gli/i*, *tri-ni-tà/in-fi-ni-ta*). Muse Sing le canta come una sillaba sola.
- **Testo sulla voce 1 del rigo.** Il testo nella seconda voce viene segnalato ma non cantato.
- **Note piccole di richiamo** (non suonate) e acciaccature vengono saltate.
- **Refusi:** il testo viene cantato così com'è scritto. Controlla accenti e apostrofi (*prostrerò*, non *prostrero*).

### Avvisi

Il riquadro giallo elenca ciò che Muse Sing non gestisce del tutto:

- **cambi di metro:** il file usa un metro solo, ma le note restano al posto giusto;
- **testo nella seconda voce del rigo;**
- **battute «simile»** (ripetizione della battuta precedente);
- **più strofe che passate:** le strofe in più non vengono cantate.

### Pronuncia

- **Italiano:**
  - la doppia «r» è sempre vibrante (*terra*);
  - la «i» semiconsonante è scritta in modo esplicito (*pietà*, *cieli*, *pieni*);
  - *Dio*, *mio* e simili hanno la «i» piena.
  - Le altre parole le converte OpenUtau.
- **Inglese:** usa il phonemizer DIFFS EN+, con correzioni per le forme arcaiche (*ask'd*, *feel'st*, *couldst*).
- **Correggere una parola in OpenUtau:** scrivi la pronuncia tra parentesi quadre nella nota, per esempio `terra[t e rr a]`.

## Registra un cantante

Questa scheda serve a costruire un cantante italiano con la voce di un corista. Si registrano circa **un'ora di frasi per ogni voce**: per esempio una femminile e una maschile. Con le registrazioni si addestra poi un modello DiffSinger da installare in OpenUtau come gli altri cantanti.

### Cosa serve

- un microfono USB o un microfono con scheda audio;
- **cuffie chiuse**, perché la melodia guida non deve finire nella registrazione. Vanno bene anche gli AirPods o altre cuffie Bluetooth **per ascoltare**, ma non il loro microfono (vedi sotto);
- una stanza silenziosa e poco riverberante: tende, divano e libri aiutano, il bagno no;
- il **consenso scritto** di chi canta: la sua voce diventa un modello, da usare solo per il coro.

### Prima sessione

1. Apri la scheda **Registra un cantante**.
2. Scrivi il **nome**, scegli la **voce** e controlla la **nota più grave** e la **più acuta** che la persona canta comodamente (Do4 è il Do centrale del pianoforte). L'estensione serve a trasporre esercizi e vocalizzi: meglio un po' stretta che forzata.
3. Premi **Nuovo cantante** e scegli dove salvare. Muse Sing crea una cartella con il nome della persona.
4. macOS chiede il permesso di usare il microfono: rispondi **Consenti**.
5. In alto a destra, nel riquadro con il microfono e le cuffie, controlla quali dispositivi usa Muse Sing. Con «automatico» registra con il microfono di sistema, ma se quello è il microfono di cuffie Bluetooth sceglie da solo il microfono del Mac (o un altro collegato).
6. Regola il guadagno del microfono: l'indicatore è sempre attivo, quindi puoi provare prima di registrare. Cantando forte deve restare verde o giallo (il segno sottile indica −12 dB, un buon picco), mai rosso.

**Perché non il microfono degli AirPods.** Quando si usa il microfono di cuffie Bluetooth, macOS le mette in modalità «telefonata»: l'audio diventa mono e di bassa qualità, inadatto a un cantante, e le cuffie si riconfigurano, cosa che può far perdere i primi colpi del conteggio. Se proprio vuoi usarlo, sceglilo dal menu: Muse Sing mostra un avviso giallo.

Il microfono resta aperto solo mentre sei nella scheda **Registra un cantante** con un cantante aperto: uscendo, l'indicatore del microfono di macOS si spegne e le cuffie tornano in alta qualità.

Le volte successive il cantante compare sotto **Riprendi**. In alternativa usa **Apri cartella…** e scegli la cartella che contiene `profile.json`.

### Le frasi da cantare

A sinistra c'è l'elenco delle frasi, diviso in gruppi:

| Gruppo | Cosa contiene | Durata |
|---|---|---|
| Esercizi · registro medio | circa 100 frasi italiane che coprono tutti i suoni, le doppie e i dittonghi | ~12 min |
| Vocalizzi | una vocale su cinque note, su tutta l'estensione | ~1 min |
| Brano · … | i brani del coro che aggiungi tu | variabile |
| Esercizi · registro grave | le stesse frasi, più in basso | ~12 min |
| Esercizi · registro acuto | le stesse frasi, più in alto | ~12 min |

Il pallino diventa verde quando la frase è registrata. Diventa giallo se la registrazione è da rifare (satura o troppo bassa). In alto vedi quante frasi e quanti minuti hai già registrato.

**Aggiungere un brano del coro:** premi «Aggiungi un brano del coro», scegli il `.mscz` e la voce da cantare (Muse Sing propone quella adatta al tipo di voce), poi «Aggiungi questa voce». Il brano viene tagliato in frasi alle pause, e i passaggi ripetuti con le stesse parole compaiono una volta sola. Il cestino accanto al nome del gruppo toglie il brano dall'elenco, ma le registrazioni restano nella cartella.

### Ascoltare la guida

Sotto il testo c'è il rullo delle note, con un righello in alto e un cursore tratteggiato che indica da dove parte l'ascolto.

- **Clic su una nota:** la senti da sola (utile per prendere l'intonazione) e il cursore si sposta al suo inizio.
- **Clic o trascinamento** sul righello o in uno spazio vuoto: sposti il cursore. Vicino all'inizio di una nota il cursore si aggancia.
- **Guida** (o G) suona la melodia dal cursore; **Pausa** la ferma lì, e la volta dopo riparte da quel punto. **⏮** (o Home) riporta il cursore all'inizio. A fine frase il cursore torna all'inizio da solo.
- **50% / 75% / 100%** rallenta la guida senza cambiarne l'intonazione, per impararla. La registrazione resta sempre alla velocità scritta.
- **Loop** ripete la guida finché non la fermi.
- Le stesse cose valgono per **Riascolto**: parte dal cursore e si mette in pausa.

### Registrare una frase

1. Ascolta la guida finché la frase è sicura.
2. Premi **Registra** (o la barra spaziatrice). La registrazione parte sempre dall'inizio della frase: quattro colpi di conteggio con un grande numero al centro, e sotto la prima nota da cantare. Il numero segue i colpi che senti, anche con il ritardo delle cuffie Bluetooth. Poi canti mentre la linea rossa scorre sulle note e la nota da cantare si accende.
3. La registrazione si ferma da sola un secondo dopo l'ultima nota. Resta in silenzio fino alla fine.
4. Premi **Riascolta** (o P). Se non va bene, **Rifai**: la nuova registrazione sostituisce la vecchia.
5. Passa alla frase successiva con **Prossima da registrare** (o Invio).

Opzioni sotto i pulsanti (Muse Sing le ricorda):

- **melodia in cuffia mentre canto:** la guida suona piano anche durante la registrazione. Se ti dà fastidio spegnila: i colpi di conteggio restano.
- **nota di partenza nel conteggio:** negli ultimi due colpi suona piano la prima nota, così parti intonato anche senza guida.
- **avanti da sola dopo una registrazione buona:** se la registrazione non satura e non è troppo bassa, dopo un attimo passa alla prossima frase da registrare. Premi «Resta qui» (o Esc) per fermarla.

**Scorciatoie**

| Tasto | Azione |
|---|---|
| Spazio | registra, oppure annulla la registrazione in corso |
| G | guida / pausa |
| P | riascolta / pausa |
| Home | cursore all'inizio della frase |
| , . | cursore sulla nota precedente o successiva |
| ← → | frase precedente o successiva |
| Invio | prossima frase da registrare |
| Esc | ferma (o resta sulla frase, se sta per passare alla prossima) |

### Se qualcosa non va con l'audio

- **Un errore sul microfono:** il messaggio dice la causa (permesso negato, dispositivo occupato o scollegato) e tra parentesi quadre il codice tecnico, utile da riferire. Premi **Riprova** nel riquadro del microfono.
- **L'indicatore resta fermo mentre canti:** in Impostazioni di Sistema → Privacy e sicurezza → Microfono attiva Muse Sing.
- **Cuffie scollegate o cambiate a metà:** compare un avviso; ricollegale e premi **Riprova**.

### Consigli per registrazioni utili

- **Canta con voce naturale**, come in coro ma da solo: niente vibrato esagerato, niente effetti.
- **Pronuncia bene le doppie e le consonanti finali**: il cantante imparerà esattamente quello che sente.
- **Tieni sempre la stessa distanza dal microfono**, circa 20–30 cm, e mettici davanti un antipop se ce l'hai.
- **Rifai una frase** se c'è una stonatura evidente, un rumore (colpo di tosse, porta, telefono) o una parola sbagliata. Piccole imprecisioni vanno bene.
- **Fai sessioni brevi**, 30–45 minuti, a voce riposata. Meglio tre sessioni buone che una lunga e stanca.

### La prova pilota

Prima di registrare tutto, fai una prova con una sola persona: **registro medio e vocalizzi**, circa 13 minuti. Con queste registrazioni si fa un addestramento di prova (pochi euro) e si ascolta il risultato. Se va bene, si registra il resto.

### Dopo le registrazioni

La cartella del cantante contiene:

```
profile.json   nome, voce, estensione
corpus.json    le frasi da cantare
takes.json     le frasi registrate
wavs/          un file WAV per frase
```

Apri la cartella dall'icona in alto a sinistra e mandala intera a chi prepara il cantante. Seguono l'etichettatura automatica, l'addestramento su una GPU in affitto e il pacchetto da installare in OpenUtau. Queste fasi non sono ancora nell'app.

## Problemi comuni

**«L'app è danneggiata e non può essere aperta».**
È il blocco di macOS per le app senza certificato Apple. Dal Terminale:

```
xattr -cr "/Applications/Muse Sing.app"
```

Lo stesso vale per OpenUtau.

**Nessun cantante nell'elenco.**
Controlla che i cantanti siano in `~/Library/OpenUtau/Singers` e che OpenUtau li veda, poi premi «aggiorna cantanti» in Muse Sing.

**Accanto al cantante c'è scritto «solo .ustx».**
Muse Sing non può usare quel cantante per generare l'audio: il motivo è scritto sotto la voce (di solito richiede parti del motore non ancora supportate). Puoi comunque usarlo nel `.ustx` con OpenUtau.

**«Genera audio» è lento.**
Il calcolo usa il processore del Mac e dura circa quanto il brano, per ogni voce. La seconda volta le voci già generate sono pronte subito.

**Il cantante pronuncia male una parola.**
Controlla che non sia un refuso nella partitura. Poi correggila in OpenUtau con la pronuncia tra parentesi quadre (vedi «Pronuncia»).

**Le voci non partono insieme o il brano è troppo corto.**
Controlla gli avvisi gialli. Se nella partitura ci sono ritornelli o salti scritti in modo insolito, segnalalo con il file `.mscz`.

**Il microfono non registra nulla.**
- Controlla l'ingresso in Impostazioni di Sistema → Suono → Ingresso.
- Controlla il permesso in Impostazioni di Sistema → Privacy e sicurezza → Microfono: Muse Sing deve essere attivo. Se l'hai negato per sbaglio, attivalo lì e riapri l'app.

**«Il segnale satura».**
Abbassa il guadagno della scheda audio o del microfono, oppure allontanati un poco. Poi rifai la frase.

**«Troppo bassa».**
Alza il guadagno o avvicinati al microfono, poi rifai la frase.

**Nella registrazione si sente la guida.**
Usa cuffie chiuse e abbassa il volume, oppure spegni «melodia in cuffia mentre canto».

## Licenze

- **TIGER DS** (tigermeat): gratuito solo per uso non commerciale, citando l'autore. Non si può ridistribuire né modificare.
- **Motore di Muse Sing:** deriva dal codice DiffSinger di OpenUtau (licenza MIT); l'export MP3 usa LAME (licenza LGPL). Dettagli nel file NOTICE del progetto.
- **Cantanti costruiti con la scheda Registra:** la voce appartiene a chi l'ha registrata. Usali solo per il coro e con il suo consenso. La parte che produce l'audio finale (il vocoder openvpi) ha licenza CC BY-NC-SA 4.0, cioè non commerciale.
- **Muse Sing:** © 2026 Emanuele Trivella, licenza MIT.
