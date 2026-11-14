# Novità

Tutte le modifiche importanti di Muse Sing. Il numero di versione si legge nell'intestazione dell'app, accanto a «Manuale».

## Non ancora rilasciato

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
<!-- le correzioni audio con cuffie Bluetooth sono da confermare su un Mac prima del rilascio -->
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
