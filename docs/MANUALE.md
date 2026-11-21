# Manuale di Muse Sing

Muse Sing fa cantare una partitura MuseScore: le voci cantano davvero le parole, non solo «oh oh». Serve soprattutto per **l'ascolto d'insieme**: sentire il brano cantato con il testo, voce per voce o tutte insieme, e portarsi a casa un MP3 per studiare.

L'app ha tre schede:

- **Canta una partitura:** crea un progetto da una partitura, genera l'audio cantato (un cantante per ogni voce, più l'accompagnamento), lo fa ascoltare con il rullo delle note e lo esporta in MP3, FLAC o WAV, anche come basi di studio per ogni voce.
- **Registra un cantante:** raccoglie le registrazioni per costruire un cantante italiano con la voce di un corista.
- **Manuale:** questa guida e le novità di ogni versione.

## Prima di iniziare

Servono due cose, da installare una volta sola: un cantante e Muse Sing. OpenUtau non serve più.

### 1. Il cantante (voicebank)

Per ora Muse Sing usa **TIGER DS v106**, che canta sia in italiano sia in inglese: https://github.com/spicytigermeat/tiger_diffsinger/releases/download/v106/TIGER_DS_v106_PACK.zip

Nel pacchetto c'è un secondo zip dentro `Voice Library/`: è quello da decomprimere, nella cartella dei cantanti di Muse Sing. Dal Terminale:

```
mkdir -p ~/Library/Application\ Support/app.musesing.desktop/Singers/Tiger
cd ~/Library/Application\ Support/app.musesing.desktop/Singers/Tiger
unzip ~/Downloads/TIGER_DS_v106.zip
```

Poi in Muse Sing premi «aggiorna cantanti». Muse Sing trova i cantanti anche nella cartella di OpenUtau (`~/Library/OpenUtau/Singers`), se c'è ancora; se ne sposti uno da lì alla cartella di Muse Sing, i progetti lo ritrovano da soli.

### 2. Muse Sing

Serve un Mac con processore Apple (M1 o successivi): sui Mac Intel funziona solo fino alla versione 0.4.0.

1. Apri il `.dmg` e trascina **Muse Sing** in Applicazioni.
2. Al primo avvio, se macOS dice che l'app è danneggiata, sbloccala una volta:

   ```
   xattr -cr "/Applications/Muse Sing.app"
   ```

**Aggiornamenti e novità.** All'avvio Muse Sing controlla se c'è una versione nuova. Se c'è, mostra le novità e chiede se aggiornare: con «Aggiorna» scarica, installa e si riavvia da sola. Il numero di versione in alto a destra apre le novità di ogni versione; un pallino viola segnala quelle che non hai ancora letto. Dopo un aggiornamento macOS può chiedere di nuovo il permesso per il microfono, una volta.

## Canta una partitura

Ogni brano diventa un **progetto** (file `.msng`): contiene una copia della partitura, i cantanti scelti, il mix e le altre impostazioni. Puoi spostarlo o mandarlo a qualcuno: si apre anche senza la partitura originale. I progetti `.musesing` delle versioni precedenti si aprono ancora: al primo salvataggio diventano `.msng` (il vecchio file resta dov'era). L'audio generato non è nel file, ma Muse Sing lo ricorda: riaprendo il progetto, «Genera audio» riusa le voci già calcolate.

### Creare e salvare un progetto

1. Apri la scheda **Canta una partitura**.
2. **Trascina** la partitura nella finestra, oppure premi **Nuovo da partitura…**. Va bene un file di MuseScore (`.mscz`) o un MusicXML (`.musicxml`, `.mxl`, `.xml`), il formato che esportano gli altri programmi di notazione. Compaiono titolo, metro, tempo, durata, le voci e il rullo con le note di tutte le parti.
3. Premi **Salva** (⌘S) e scegli dove mettere il progetto. **Salva con nome…** (⇧⌘S) ne fa una copia.
4. Per riaprirlo: fai doppio clic sul `.msng` nel Finder, trascinalo nella finestra, premi **Apri progetto…** o sceglilo tra i **progetti recenti** nella pagina iniziale.

Il pallino viola accanto al nome indica modifiche non salvate: Muse Sing lo ricorda prima di chiudere o di aprire un altro progetto.

**Partitura corretta in MuseScore?** Premi **Ricarica partitura…** e scegli il file aggiornato: le scelte delle voci che ci sono ancora restano, poi genera di nuovo.

### Voci, cantanti e impostazioni

- **Lingua:** italiano, inglese o **latino (ecclesiastico)**; viene riconosciuta da sola, ma puoi cambiarla.
- **Intonazione:**
  - **naturale:** il cantante decide come attaccare e legare le note, con un po' di vibrato. Suona più umano;
  - **esatta:** la voce sta ferma sulle note scritte. Utile per controllare l'intonazione della propria parte.
- **Velocità:** 100% è il tempo scritto; 90%, 80%, 70% o 60% rallentano tutto il brano (voci e accompagnamento) per studiare i passaggi difficili. Cambiare la velocità richiede di generare di nuovo; ogni velocità resta in memoria.
- **Voci:** spunta quelle da cantare e scegli il **cantante** di ognuna. Se il cantante ha più colori di voce (Tiger ne ha diversi), accanto compare la scelta del colore. Muse Sing ricorda il cantante di ogni voce (Soprano, Tenore…) da un brano all'altro.
- **La mia voce:** il pulsante accanto al nome evidenzia quella voce nel rullo.
- **Accompagnamento:** se la partitura ha parti senza testo (organo, pianoforte), Muse Sing le suona con un suono d'organo semplice. Si attiva o disattiva con la casella.

### Generare e ascoltare

1. Premi **Genera audio**. Il Mac calcola le voci una dopo l'altra: per un brano di 3-4 minuti con quattro voci servono alcuni minuti. Puoi annullare; la frase in corso finisce prima di fermarsi.
2. Premi **Ascolta**. Con la barra o con un clic sul rullo scegli da dove partire; durante l'ascolto il rullo scorre con la linea rossa.
3. **Ripetere un tratto:** trascina sul righello in cima al rullo (i minuti) per scegliere un tratto, che si colora di giallo: l'ascolto lo ripete finché non premi Ferma. La × accanto al tratto, sopra, lo toglie.
4. **Mix** di ogni voce e dell'accompagnamento: volume, posizione a sinistra o a destra (pan), «muta» per escluderla, «solo» per sentire solo le tracce con «solo» acceso. Le modifiche si sentono subito, senza rigenerare. Doppio clic su un cursore lo riporta a zero.

Le voci già generate con le stesse scelte vengono riusate: se cambi il cantante di una sola voce, si rigenera solo quella, e dopo un ritocco solo le frasi toccate. Se cambi voci, cantanti, lingua, intonazione, velocità o ritocchi dopo aver generato, Muse Sing chiede di generare di nuovo.

**Avvisi di pronuncia.** Sotto il lettore, «avvisi di pronuncia» elenca le parole che il cantante non sa dire del tutto (un suono che non ha). Di solito la parola si sente lo stesso, con quel suono saltato.

### Ritocchi

Dopo aver generato l'audio, il riquadro **Ritocchi** sotto il rullo mostra una voce alla volta (sceglila in alto a sinistra; parte da «la mia voce»): le note in grigio, in viola l'intonazione davvero cantata, sotto le parole. I ritocchi si salvano nel progetto. Se ricarichi la partitura:
- i ritocchi di una parola restano legati alla sua nota (battuta, passata, posizione nella battuta). Se aggiungi o togli battute prima di quella parola, al suo posto c'è un'altra parola: Muse Sing non applica il ritocco e lo segnala tra gli avvisi;
- le curve restano alla stessa posizione nel brano: se aggiungi o togli battute prima, si spostano con le note solo da lì in avanti. In quel caso controllale.

**Applica e ascolta** rigenera solo le frasi cambiate (qualche secondo) e fa partire l'ascolto poco prima del punto ritoccato.

- **Pronuncia:** clicca una parola. Nel pannello vedi i fonemi che il cantante canta (per esempio *tx oh gl gl iy*):
  - **cambiali** scrivendo nella casella o cliccando i fonemi del cantante (in viola le vocali), separati da spazi. Si possono usare anche i simboli italiani del dizionario (*t*, *EE*, *JJ*…);
  - **solo qui** cambia solo questa volta; **ogni volta che c'è «parola»** vale per tutto il brano (in quella voce);
  - **tempi:** trascina l'inizio di un fonema nella barretta sotto, per esempio per anticipare una consonante. Prima applica i fonemi, poi sistema i tempi;
  - **Ripristina questa parola** torna alla pronuncia del dizionario.
- **Intonazione:** disegna con il mouse nella fascia sotto le note: in alto la voce sale, in basso scende (100 cent = un semitono, al massimo ±2 semitoni). **Vibrato sulle note**, poi clic su una nota, aggiunge un vibrato leggero nella seconda parte della nota.
- **Dinamica:** più forte o più piano in un punto (da −24 a +12 dB). Anche qui serve «Applica e ascolta», ma è immediato: la voce non si rigenera.
- **Timbro** (sperimentale): più scuro o più chiaro. Con TIGER fino a ±0,3 cambia poco e oltre la voce si rovina; con altri cantanti potrebbe andare meglio.
- **Consonanti:** più veloci o più lente (fino a 1,4 volte; oltre, con TIGER la voce perde l'intonazione).

Nelle curve, **⌥** mentre disegni (o il pulsante **Gomma**) cancella; **Togli tutta la curva** la azzera.

Nota: l'intonazione naturale la sceglie il modello del cantante, che ogni volta la canta un po' diversa. Muse Sing ricorda quella della prima generazione di ogni frase, così un ritocco cambia solo ciò che hai disegnato.

### Esportare

- **Esporta…:** il brano con il mix che senti, in MP3 (per il telefono), FLAC (senza perdita di qualità) o WAV.
- **Basi di studio…:** scegli una cartella e Muse Sing crea un MP3 per ogni voce (*Santo - Soprano.mp3*, *Santo - Contralto.mp3*…), con quella voce in primo piano e le altre più basse (di quanto, lo scegli accanto: −10 dB è un buon inizio). L'accompagnamento resta come nel mix. Sono le classiche basi da mandare al coro.

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
  - *Dio*, *mio* e simili hanno la «i» piena;
  - le altre parole vengono dal dizionario italiano o, se mancano, da un modello che ne indovina la pronuncia.
- **Inglese:** dizionario ARPAbet+, con correzioni per le forme arcaiche (*ask'd*, *feel'st*, *couldst*).
- **Latino:** pronuncia ecclesiastica («all'italiana»), cantata con i suoni dell'italiano, quindi con qualunque cantante che canta in italiano:
  - «c» e «g» dolci davanti a *e*, *i*, *ae*, *oe* (*pacem*, *regina*, *caeli*); *sc* = «sc» di *scena* (*descendit*, *suscipe*); *xc* = «ksc» (*excelsis*);
  - *ae* e *oe* = «e»; *gn* = «gn» (*agnus*); *qu* e *ngu* = «kw», «ngw» (*qui*, *sanguis*);
  - *ti* davanti a vocale = «tsi» (*gratia*, *etiam*), ma non dopo *s*, *t*, *x* (*hostia*);
  - *h* muta, tranne *mihi* e *nihil* («miki», «nikil»); la doppia «r» è vibrante (*terra*); *j* e *i* davanti a vocale = «i» semiconsonante (*Iesu*, *eius*); *ex* davanti a vocale = «egz» (*exaudi*); *z* = «dz»;
  - ogni sillaba scritta sotto una nota canta una sola vocale: in *lau-da-mus* la «u» di *lau* scivola sulla «a».
  - Le «e» e le «o» sono chiuse e la «s» tra due vocali è sorda; se il tuo coro le canta diversamente, correggi la parola nei **Ritocchi** «in tutto il brano».
- Una parola detta male si corregge nei **Ritocchi** (vedi sopra).

## Registra un cantante

Questa scheda serve a costruire un cantante italiano con la voce di un corista. Si registrano circa **un'ora di frasi per ogni voce**: per esempio una femminile e una maschile. Con le registrazioni si addestra poi un modello DiffSinger da copiare nella cartella dei cantanti di Muse Sing.

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

Apri la cartella dall'icona in alto a sinistra e mandala intera a chi prepara il cantante. Seguono l'etichettatura automatica, l'addestramento su una GPU in affitto e il pacchetto da copiare nella cartella dei cantanti di Muse Sing. Queste fasi non sono ancora nell'app.

## Problemi comuni

**«L'app è danneggiata e non può essere aperta».**
È il blocco di macOS per le app senza certificato Apple. Dal Terminale:

```
xattr -cr "/Applications/Muse Sing.app"
```

**Nessun cantante nell'elenco.**
Controlla che il cantante sia decompresso in `~/Library/Application Support/app.musesing.desktop/Singers` (in una sua cartella, con il file `character.txt`), poi premi «aggiorna cantanti».

**Accanto al cantante c'è scritto «non supportato».**
Muse Sing non può usare quel cantante: il motivo è scritto sotto la voce (di solito richiede parti del motore non ancora supportate).

**«Genera audio» è lento.**
Il calcolo usa il processore del Mac e dura circa quanto il brano, per ogni voce. La seconda volta le voci già generate sono pronte subito.

**Il cantante pronuncia male una parola.**
Controlla che non sia un refuso nella partitura. Puoi anche scrivere la pronuncia tra parentesi quadre nel testo della nota in MuseScore, per esempio `terra[t e rr a]`, e ricaricare la partitura; l'editor di pronuncia dentro Muse Sing arriverà in una prossima versione.

**Le voci non partono insieme o il brano è troppo corto.**
Controlla gli avvisi gialli. Se nella partitura ci sono ritornelli o salti scritti in modo insolito, segnalalo con il file della partitura. Con un MusicXML, prova anche a esportarlo di nuovo dal programma di notazione: alcuni programmi scrivono D.C., D.S. e Coda in modo incompleto.

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
