# Manuale di Muse Sing

Muse Sing fa cantare una partitura MuseScore: le voci cantano davvero le parole, non solo «oh oh». Serve soprattutto per **l'ascolto d'insieme**: sentire il brano cantato con il testo, voce per voce o tutte insieme, e portarsi a casa un MP3 per studiare.

L'app ha tre schede:

- **Canta una partitura:** crea un progetto da una partitura, genera l'audio cantato (un cantante per ogni voce, più l'accompagnamento), lo fa ascoltare con il rullo delle note e lo esporta in MP3, FLAC o WAV, anche come basi di studio per ogni voce.
- **Registra un cantante:** raccoglie le registrazioni per costruire un cantante italiano con la voce di un corista.
- **Manuale:** questa guida e le novità di ogni versione.

## Prima di iniziare

Servono due cose, da installare una volta sola: un cantante e Muse Sing. OpenUtau non serve più.

### 1. Il cantante (voicebank)

Per ora Muse Sing usa **TIGER DS v106**, che canta in italiano, latino e inglese. La licenza non permette di includerlo nell'app, ma Muse Sing lo scarica per te dal sito dell'autore: apri una partitura e, se non c'è nessun cantante, premi **Scarica Tiger** (circa 550 MB; ci vogliono alcuni minuti). Quando ha finito, Tiger compare tra i cantanti.

Per installarlo a mano: il pacchetto è https://github.com/spicytigermeat/tiger_diffsinger/releases/download/v106/TIGER_DS_v106_PACK.zip e dentro `Voice Library/` c'è un secondo zip: è quello da decomprimere, nella cartella dei cantanti di Muse Sing. Dal Terminale:

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

Ogni brano diventa un **progetto** (file `.msng`): contiene una copia della partitura, i cantanti scelti, il mix e le altre impostazioni. Puoi spostarlo o mandarlo a qualcuno: si apre anche senza la partitura originale. I progetti `.musesing` delle versioni precedenti si aprono ancora: al primo salvataggio diventano `.msng` (il vecchio file resta dov'era). Salvando, nel file vanno anche le **voci generate** (circa 2–3 MB per minuto per voce, senza perdita di qualità): riaprendo il progetto, anche su un altro Mac, tornano pronte da sole (per qualche secondo compare «Ricarico le voci già generate…») e si apre subito la scheda Studia. Le parti suonate e l'accompagnamento si rifanno da soli in pochi secondi. Se dopo l'ultima generazione hai cambiato qualcosa che tocca le voci, nel file resta l'audio di prima finché non generi di nuovo e salvi. Per un file più leggero togli la spunta «Salva l'audio nel progetto» nella scheda **Info**: le voci si ritrovano lo stesso finché sono nella cache di Muse Sing. Nel progetto si salvano tutte le scelte: cantanti, colori, coristi, lingua, intonazione, velocità, tonalità, strumenti, ritocchi, mixer, metronomo, ambiente, e anche «Io canto», la tua voce nella base e il volume delle altre voci nelle basi. All'avvio Muse Sing riapre da solo l'ultimo progetto usato.

### Creare e salvare un progetto

1. Apri la scheda **Canta una partitura**.
2. **Trascina** la partitura nella finestra, oppure premi **Nuovo da partitura…**. Va bene un file di MuseScore (`.mscz`) o un MusicXML (`.musicxml`, `.mxl`, `.xml`), il formato che esportano gli altri programmi di notazione. Compaiono titolo, metro, tempo, durata, le voci e il rullo con le note di tutte le parti.
3. Premi **Salva** (⌘S) e scegli dove mettere il progetto. **Salva con nome…** (⇧⌘S) ne fa una copia.
4. Per riaprirlo: fai doppio clic sul `.msng` nel Finder, trascinalo nella finestra, premi **Apri progetto…** o sceglilo tra i **progetti recenti** nella pagina iniziale.

Il pallino viola accanto al nome indica modifiche non salvate: Muse Sing lo ricorda prima di chiudere o di aprire un altro progetto.

**Partitura corretta in MuseScore?** Premi **Ricarica partitura…** e scegli il file aggiornato: le scelte delle voci che ci sono ancora restano, poi genera di nuovo.

### Le schede

Il progetto ha due schede di lavoro, più **Info**:
- **1 · Prepara le voci:** tutto ciò che serve a generare l'audio, cioè lingua, intonazione, velocità, tonalità, cantanti, strumenti e Ritocchi. Ogni cambiamento qui richiede di generare di nuovo.
- **2 · Studia:** si apre quando le voci sono pronte. Contiene il mixer, l'ascolto con il rullo, **Canta e controlla** e l'esportazione. Qui non si rigenera niente.
- **Info:** il riepilogo del brano (voci, cantanti, strumenti, impostazioni), quanto pesa il file del progetto (partitura, impostazioni, audio) e la cache di Muse Sing. Da qui:
  - **Salva l'audio nel progetto** (spunta) decide se le voci vanno nel file;
  - **Rimuovi l'audio dal progetto** toglie le voci dal file e lo salva subito, per liberare spazio;
  - **Svuota la cache** cancella le voci che Muse Sing tiene da parte per tutti i progetti (fino a 2 GB). I progetti con l'audio salvato restano pronti, gli altri vanno rigenerati.

Finita la generazione, Muse Sing passa da solo a **Studia**. Se poi cambi qualcosa in **Prepara le voci**, in cima a **Studia** compare il pulsante per generare di nuovo.

### 1 · Prepara le voci

- **Lingua:** italiano, inglese o **latino (ecclesiastico)**; viene riconosciuta da sola, ma puoi cambiarla.
- **Intonazione:**
  - **naturale:** il cantante decide come attaccare e legare le note, con un po' di vibrato. Suona più umano;
  - **esatta:** la voce sta ferma sulle note scritte. Utile per controllare l'intonazione della propria parte.
- **Velocità:** 100% è il tempo scritto; 90%, 80%, 70% o 60% rallentano tutto il brano (voci e accompagnamento) per studiare i passaggi difficili. Cambiare la velocità richiede di generare di nuovo; ogni velocità resta in memoria.
- **Segni della partitura** (sotto Velocità, acceso di solito): Muse Sing esegue quello che è scritto oltre alle note:
  - **dinamiche e forcelle** (*pp*, *p*, *mf*, *f*… e crescendo/diminuendo) alzano e abbassano il volume di ogni voce;
  - **respiri** (la virgola): la nota prima si accorcia un poco e il cantante prende fiato;
  - **corone**: il tempo rallenta su quella nota, per tutte le voci insieme;
  - **rit., rall., allarg., accel.** e **a tempo**: il tempo cambia gradualmente, al massimo per due battute.

  Togli la spunta per un'esecuzione piatta, a volume e tempo costanti. Anche il rullo segue i segni, così il cursore resta sulla musica.
- **Tonalità:** sposta tutto il brano (voci, strumenti, conteggio e rullo) fino a 6 semitoni sopra o sotto, per adattarlo alla tua voce. Come la velocità, richiede di generare di nuovo.
- **Voci:** spunta quelle da cantare e scegli il **cantante** di ognuna. Se il cantante ha più colori di voce (Tiger ne ha diversi), accanto compare la scelta del colore. Muse Sing ricorda il cantante di ogni voce (Soprano, Tenore…) da un brano all'altro.
- **Coristi:** con 2 o 3 coristi la voce è cantata da una piccola sezione invece che da un solista: ogni corista ha un altro colore di voce e canta in modo leggermente diverso, come in un coro vero. La generazione dura 2 o 3 volte tanto (le voci già generate si riusano).
- **Cantata o suonata:** ogni voce può essere **suonata** al pianoforte invece che cantata, per imparare le note senza le parole. È istantanea e non serve un cantante.
- **Accompagnamento:** se la partitura ha parti senza testo (organo, pianoforte), Muse Sing le suona tutte, accordi compresi. Si attiva o disattiva con la casella. All'inizio suona un organo semplice; con **Scarica strumenti veri** (31 MB, una volta sola) Muse Sing scarica gli strumenti di GeneralUser GS e puoi scegliere per ogni parte organo a canne, armonium, pianoforte, archi, flauto, chitarra o arpa. Lo strumento proposto dipende dal nome della parte. Dopo il download o un cambio di strumento serve **Genera audio** (le voci già cantate si riusano, quindi è veloce).

**Genera le voci.** Premi **Genera audio**. Il Mac calcola le voci una dopo l'altra: per un brano di 3-4 minuti con quattro voci servono alcuni minuti. Puoi annullare; la frase in corso finisce prima di fermarsi.

Le voci già generate con le stesse scelte vengono riusate: se cambi il cantante di una sola voce, si rigenera solo quella, e dopo un ritocco solo le frasi toccate.

**Avvisi di pronuncia.** Sotto il pulsante, «avvisi di pronuncia» elenca le parole che il cantante non sa dire del tutto (un suono che non ha). Di solito la parola si sente lo stesso, con quel suono saltato.

### 2 · Studia

- **Io canto:** scegli la tua voce. È evidenziata nel rullo ed è quella che canti in **Canta e controlla**.
- **Mixer:** per ogni voce, per l'accompagnamento e per il metronomo ci sono:
  - il **volume** (doppio clic: 0 dB);
  - la posizione a sinistra o a destra (**pan**);
  - **accesa / spenta**, per escludere la traccia;
  - **solo**, per sentire solo le tracce con «solo» acceso.

  Il mix vale per l'ascolto, per Canta e controlla e per i file esportati, e si sente subito, senza rigenerare.
  - **Metronomo:** con la sua casella si sente un colpo per ogni battito, più forte sul primo della battuta; nei 6/8, 9/8 e 12/8 batte le semiminime puntate. Ha volume e pan come le altre tracce e si accende o spegne senza rigenerare.
  - **Ambiente:** il riverbero di una stanza, di una chiesa o di una cattedrale, sentito in ascolto e messo nei file esportati e nelle basi di studio. «Nessuno» lascia la voce secca, com'era prima. Sotto, lo slider **Riverbero** regola quanto se ne sente: 50% è il valore normale, 0% lo toglie, 100% lo raddoppia (doppio clic: torna al 50%). Si può cambiare in qualsiasi momento senza generare di nuovo.

**Ascoltare.**
1. Premi **Ascolta**. Con la barra o con un clic sul rullo scegli da dove partire; durante l'ascolto il rullo scorre con la linea rossa. Il righello in cima mostra le battute con gli stessi numeri della partitura (anche nei ritornelli, dove i numeri si ripetono), e in viola il metro dove cambia.
   Con **conteggio** acceso, prima di partire si sente una battuta di colpi al tempo del punto di partenza (due battute nei tempi in 6/8), con l'accordo delle voci sotto gli ultimi due colpi. Non vale quando ripeti un tratto.
3. **Ripetere un tratto:** trascina sul righello in cima al rullo per scegliere un tratto, che si colora di giallo: l'ascolto lo ripete finché non premi Ferma. La × accanto al tratto, sopra, lo toglie.

### Canta e controlla

Per provare la tua parte e vedere come è andata, nota per nota.

1. In **Studia** scegli la tua voce in **Io canto**.
2. **Mettiti le cuffie** e premi **Accendi il microfono**. Senza cuffie il microfono sente anche le altre voci e il risultato non vale.
   - Scegli microfono e cuffie: sono gli stessi di **Registra un cantante**.
   - Parla: la barra del livello deve muoversi. Se resta ferma, vedi «Il microfono non sente» in Problemi comuni.
   - La prima volta il Mac chiede il permesso per il microfono.
   - Il microfono si spegne quando esci da Studia o con «Spegni il microfono».
3. Nel **Mixer** abbassa o spegni le voci che non vuoi sentire mentre canti. Durante la prova il mixer è bloccato.
4. Scegli cosa cantare. Di solito si parte dal cursore e si arriva alla fine; se trascini sul righello del rullo, invece, canti solo quel tratto.
5. Scegli come sentire **la tua voce nella base**: **spenta** (canti da solo sopra le altre), **piano** (una traccia guida bassa) o **come nel mixer**.
6. Premi **Canta**. Prima senti la battuta di conteggio, poi la musica, e intanto canti la tua parte. **Ferma** chiude prima la prova; si valuta quello che hai cantato fin lì.

Dopo la prova le note della tua parte nel rullo cambiano colore:
- **verde:** intonata, entro un quarto di semitono (25 cent);
- **giallo:** quasi, tra 25 e 50 cent;
- **rosso:** stonata, oltre 50 cent;
- **bordo rosso:** non cantata.

La **linea azzurra** è la tua voce. Il riepilogo dice quante note sono intonate, se tendi a calare o a crescere e quali battute riprendere. **Riascolta** fa sentire la tua registrazione sopra la base.

Cosa sapere:
- Ogni nota si valuta sulla parte centrale, senza l'attacco.
- L'ottava non conta: un uomo che canta la parte del soprano un'ottava sotto è intonato.
- Le note molto brevi non si valutano.
- Muse Sing compensa da solo un piccolo ritardo del microfono o delle cuffie Bluetooth.
- La registrazione resta in memoria solo fino alla prova successiva e non si salva nel progetto. L'ultima prova resta anche in un file, `ultima-prova.wav`, nella cartella cache di Muse Sing (`~/Library/Caches/app.musesing.desktop`), utile per segnalare un problema.

### Ritocchi

Dopo aver generato l'audio, il riquadro **Ritocchi** in **Prepara le voci** mostra una voce alla volta (sceglila in alto a sinistra; parte da «la mia voce»): le note in grigio, in viola l'intonazione davvero cantata, sotto le parole. I ritocchi si salvano nel progetto. Se ricarichi la partitura:
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
- **Basi di studio…:** scegli una cartella e Muse Sing crea un MP3 per ogni voce (*Santo - Soprano.mp3*, *Santo - Contralto.mp3*…), con quella voce in primo piano e le altre più basse (di quanto, lo scegli accanto: −10 dB è un buon inizio). L'accompagnamento resta come nel mix. Con **conteggio** acceso, ogni base comincia con la battuta di conteggio. Sono le classiche basi da mandare al coro.

  Nel menu accanto scegli il tipo di base:
  - **voce in evidenza:** come sopra;
  - **senza la propria voce:** tutte le altre voci normali e la propria muta, per cantarci sopra la tua parte (*Santo - senza Soprano.mp3*);
  - **entrambe:** i due file per ogni voce.

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

### Il microfono non sente (Canta e controlla, Registra)

Se la barra del livello resta ferma quando parli, il Mac non passa il suono a Muse Sing. Muse Sing lo segnala anche mentre canti.
1. Apri **Impostazioni di Sistema → Privacy e sicurezza → Microfono** e attiva **Muse Sing**.
2. Spegni e riaccendi il microfono in Muse Sing, oppure chiudi e riapri l'app.
3. Prova a scegliere un altro microfono nel menu (per esempio quello del Mac invece di quello delle cuffie).

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
