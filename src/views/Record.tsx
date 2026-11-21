import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, FolderOpen, Headphones, Loader2, Mic, Music, UserPlus } from "lucide-react";
import { useEffect, useState } from "react";
import { errorText, noteName, voiceApi, VOICE_TYPES, type RecentVoice, type VoiceProfile, type VoiceState } from "../lib/api";
import { ErrorBox, Session } from "./record/Session";

export default function Record({ active }: { active: boolean }) {
  const [voice, setVoice] = useState<VoiceState | null>(null);
  const [error, setError] = useState<string | null>(null);

  return (
    <main className={`min-h-0 flex-1 ${active ? "flex" : "hidden"} flex-col`}>
      {voice ? (
        <Session voice={voice} setVoice={setVoice} active={active} error={error} setError={setError} />
      ) : (
        <div className="space-y-4 overflow-auto p-5">
          <Start onOpen={setVoice} setError={setError} />
          <ErrorBox error={error} onClose={() => setError(null)} />
        </div>
      )}
    </main>
  );
}

// ------------------------------------------------------------------------------------------------
// Start: recent singers, open, create

function Start({ onOpen, setError }: { onOpen: (v: VoiceState) => void; setError: (e: string | null) => void }) {
  const [recent, setRecent] = useState<RecentVoice[]>([]);
  const [voiceType, setVoiceType] = useState("soprano");
  const [name, setName] = useState("");
  const [low, setLow] = useState(VOICE_TYPES.soprano.low);
  const [high, setHigh] = useState(VOICE_TYPES.soprano.high);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    voiceApi.recent().then(setRecent).catch(() => {});
  }, []);

  const run = async (f: () => Promise<VoiceState>) => {
    setError(null);
    setBusy(true);
    try {
      onOpen(await f());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const openFolder = async () => {
    const dir = await open({ directory: true, title: "Cartella del cantante (quella con profile.json)" });
    if (typeof dir === "string") run(() => voiceApi.open(dir));
  };

  const create = async () => {
    const parent = await open({ directory: true, title: "Dove salvo le registrazioni? (verrà creata una sottocartella)" });
    if (typeof parent !== "string") return;
    const profile: VoiceProfile = { name: name.trim(), voice: voiceType, low, high };
    run(() => voiceApi.create(parent, profile));
  };

  const pickType = (t: string) => {
    setVoiceType(t);
    setLow(VOICE_TYPES[t].low);
    setHigh(VOICE_TYPES[t].high);
  };

  const notes = Array.from({ length: 50 }, (_, i) => 36 + i);

  return (
    <>
      <section className="card space-y-2">
        <div className="text-base font-semibold">Costruisci un cantante con la tua voce</div>
        <p className="text-zinc-600 dark:text-zinc-400">
          Muse Sing ti fa cantare una serie di frasi brevi, con note e parole già scritte: esercizi che coprono tutti i suoni
          dell'italiano, vocalizzi su tutta l'estensione e i brani del vostro coro. Con circa un'ora di registrazioni per voce si
          addestra un cantante DiffSinger italiano da usare in Muse Sing.
        </p>
        <ul className="grid gap-1 text-zinc-600 sm:grid-cols-2 dark:text-zinc-400">
          <li className="flex gap-2"><Headphones className="h-4 w-4 shrink-0 text-violet-600" /> Cuffie chiuse: la guida non deve finire nel microfono.</li>
          <li className="flex gap-2"><Mic className="h-4 w-4 shrink-0 text-violet-600" /> Stanza silenziosa e poco riverberante, sempre alla stessa distanza dal microfono.</li>
          <li className="flex gap-2"><Music className="h-4 w-4 shrink-0 text-violet-600" /> Voce naturale e riposata, senza effetti né compressione.</li>
          <li className="flex gap-2"><CheckCircle2 className="h-4 w-4 shrink-0 text-violet-600" /> Meglio più sessioni brevi: si riprende da dove ci si era fermati.</li>
        </ul>
      </section>

      {recent.length > 0 && (
        <section className="card">
          <div className="label">Riprendi</div>
          <div className="flex flex-wrap gap-2">
            {recent.map((r) => (
              <button key={r.dir} className="btn" disabled={busy} onClick={() => run(() => voiceApi.open(r.dir))} title={r.dir}>
                <Mic className="h-4 w-4" /> {r.name}
              </button>
            ))}
          </div>
        </section>
      )}

      <section className="card grid gap-4 sm:grid-cols-[1fr_auto] sm:items-end">
        <div className="grid gap-3 sm:grid-cols-4">
          <div className="sm:col-span-2">
            <label className="label">Nome del cantante</label>
            <input className="field" value={name} onChange={(e) => setName(e.target.value)} placeholder="es. Chiara (soprano)" />
          </div>
          <div className="sm:col-span-2">
            <label className="label">Voce</label>
            <select className="field" value={voiceType} onChange={(e) => pickType(e.target.value)}>
              {Object.entries(VOICE_TYPES).map(([k, v]) => (
                <option key={k} value={k}>
                  {v.label}
                </option>
              ))}
            </select>
          </div>
          <div className="sm:col-span-2">
            <label className="label">Nota più grave comoda</label>
            <select className="field" value={low} onChange={(e) => setLow(+e.target.value)}>
              {notes.map((n) => (
                <option key={n} value={n}>
                  {noteName(n)}
                </option>
              ))}
            </select>
          </div>
          <div className="sm:col-span-2">
            <label className="label">Nota più acuta comoda</label>
            <select className="field" value={high} onChange={(e) => setHigh(+e.target.value)}>
              {notes.map((n) => (
                <option key={n} value={n}>
                  {noteName(n)}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="flex flex-col gap-2">
          <button className="btn-primary" disabled={busy || !name.trim() || high - low < 12} onClick={create}>
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <UserPlus className="h-4 w-4" />} Nuovo cantante
          </button>
          <button className="btn justify-center" disabled={busy} onClick={openFolder}>
            <FolderOpen className="h-4 w-4" /> Apri cartella…
          </button>
        </div>
        <p className="text-xs text-zinc-500 sm:col-span-2">
          L'estensione serve a trasporre esercizi e vocalizzi: scegli note che canti senza fatica (Do4 = il Do centrale del pianoforte).
          La voce registrata diventa un modello: serve il consenso scritto di chi canta, e il cantante resta per uso del coro.
        </p>
      </section>
    </>
  );
}
