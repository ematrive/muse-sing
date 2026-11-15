# Novità

Tutte le modifiche importanti di Muse Sing. Il numero di versione si legge nell'intestazione dell'app, accanto a «Manuale».

## Non ancora rilasciato

### Nuovo
- **Muse Sing canta da solo, senza OpenUtau.** La scheda si chiama ora «Canta una partitura»:
  - un cantante (e un colore di voce) per ogni voce, ricordato da un brano all'altro;
  - **Genera audio** con avanzamento voce per voce, annullabile; le voci già generate si riusano;
  - intonazione **naturale** (decide il cantante) o **esatta** (ferma sulle note);
  - **ascolto** con barra di posizione e **mix** per voce (volume, pan, muta) che si sente subito;
  - **export** in MP3, FLAC o WAV.
- Il `.ustx` per OpenUtau resta, ora con il cantante scelto per ogni voce.
- Con l'italiano Tiger dice anche «gn» e le «i» semiconsonanti.

### Corretto
- Sul Mac l'app si chiudeva durante «Genera audio»: ONNX Runtime usava da solo l'acceleratore CoreML di Apple, che andava in crash su alcune frasi. Ora i modelli girano sempre sul processore.
- L'app si chiudeva generando brani senza pause (per esempio «Chi è quel bimbo?»): le voci venivano calcolate in un pezzo unico e la memoria finiva. Ora le frasi più lunghe di circa 10 secondi si dividono tra una parola e l'altra.
- Sul Mac le parole fuori dal dizionario davano «fonemi non riconosciuti»: le ottimizzazioni di ONNX Runtime rompevano, sui processori Apple, il modello che ne indovina la pronuncia. Ora il modello gira senza ottimizzazioni e, se non funzionasse lo stesso, per l'italiano subentra una regola di conversione.
- Brani con più strofe: dove una strofa tiene la sillaba su più note, Muse Sing prendeva la sillaba della prima strofa (*u‑na* diventava «ume na», *l'asinello* «lasineltalo»). Ora la nota continua la sillaba della strofa giusta; il testo scritto una volta sola (ritornello) vale ancora per tutte le strofe.

## 0.4.0 — 15 novembre 2026

### Nuovo
- **Aggiornamenti dall'app:** all'avvio Muse Sing controlla se c'è una versione nuova, mostra le novità e, se vuoi, si aggiorna e si riavvia da sola.
- Un pallino accanto al numero di versione segnala le novità non ancora lette.

### Corretto
- Passando alla 0.3.0 l'app dimenticava i cantanti recenti e i voicebank scelti per ogni lingua: ora li recupera dalla versione precedente. Le registrazioni non erano state toccate.

## 0.3.0 — 15 novembre 2026 (pre-release)

### Nuovo
- **Registra un cantante: guida interattiva.**
  - Clic su una nota del rullo per sentirla; clic o trascinamento per spostare il cursore.
  - La guida va in pausa e riparte dal cursore; si può rallentare (50% e 75%) e ripetere in loop. Anche il riascolto parte dal cursore.
  - La nota che suona (o che stai cantando) si accende sul rullo.
- **Nota di partenza nel conteggio:** negli ultimi due colpi suona piano la prima nota della frase.
- **Avanti da sola** alla prossima frase dopo una registrazione buona (si attiva sotto i pulsanti).
- **Riquadro microfono e cuffie:** scelta dei dispositivi, livello del microfono sempre visibile, avviso se si registra con il microfono di cuffie Bluetooth. Con gli AirPods connessi Muse Sing registra con il microfono del Mac, così le cuffie restano in alta qualità.
- Conteggio con i pallini dei colpi e la prima nota da cantare.

### Corretto
- A volte il conteggio si sentiva solo dal «2», soprattutto con cuffie Bluetooth: microfono e cuffie ora si aprono una volta sola all'inizio della sessione, e conteggio e linea rossa seguono l'audio che esce davvero.
- Errori saltuari di connessione al microfono: l'apertura riprova da sola, e il messaggio spiega la causa.
- Le registrazioni restano allineate anche con il ritardo delle cuffie Bluetooth.
- Nell'elenco delle frasi, il testo che scorreva si vedeva sopra e dietro l'intestazione del gruppo.
- Avviare la guida e subito dopo registrare poteva lasciare i pulsanti in uno stato sbagliato.

## 0.2.0 — 8 novembre 2026

### Nuovo
- **Registra un cantante.** Una nuova scheda per raccogliere le registrazioni da cui costruire un cantante DiffSinger italiano con la voce di un corista.
  - Frasi da cantare già pronte: circa 100 frasi italiane (anche di testo liturgico) che coprono tutti i suoni, le doppie e i dittonghi, in tre registri, più 7 vocalizzi su tutta l'estensione.
  - «Aggiungi un brano del coro»: le voci dei vostri brani MuseScore diventano frasi da cantare, tagliate alle pause.
  - Per ogni frase: testo, rullo delle note e nomi delle note. Quattro colpi di conteggio, melodia guida in cuffia (si può spegnere), stop automatico.
  - Indicatore del livello del microfono e avvisi se il segnale satura o è troppo basso.
  - Ripresa da dove ci si era fermati e scorciatoie da tastiera.
  - Registrazioni WAV mono a 24 bit, salvate nella cartella del cantante.
- **Manuale utente** dentro l'app, con questa pagina delle novità.

### Modificato
- La finestra è più grande e ha tre schede: *Da MuseScore a OpenUtau*, *Registra un cantante*, *Manuale*.
- Al primo avvio macOS chiede il permesso di usare il microfono.

## 0.1.0 — 7 novembre 2026

Prima versione.

- Da un file MuseScore 4 (`.mscz`) crea un progetto OpenUtau (`.ustx`) in cui le voci cantano le parole.
- Srotola ritornelli, finali 1./2. e D.C./D.S. come la riproduzione di MuseScore. Alla seconda passata usa la seconda strofa.
- Italiano:
  - la doppia «r» è forzata (*terra* non diventa «tera»);
  - la «i» semiconsonante è scritta in modo esplicito (*pietà* non diventa «gietà»);
  - *Dio* e *mio* hanno la «i» piena.
- Inglese con il phonemizer DIFFS EN+, comprese le forme arcaiche (*ask'd*, *feel'st*, *couldst*…).
- Riconosce la lingua da solo e trova i cantanti installati in OpenUtau, ricordando la scelta per ogni lingua.
- Avvisa di ciò che non gestisce: cambi di metro, testo nella seconda voce del rigo, battute «simile», strofe in più.
