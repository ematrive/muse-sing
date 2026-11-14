import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, Headphones, Loader2, Mic, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import type { AudioStatus, Devices } from "../../lib/api";

export type MicState =
  | { state: "opening" }
  | { state: "ok"; status: AudioStatus }
  | { state: "error"; error: string };

export function LevelMeter({ level, live = true }: { level: number; live?: boolean }) {
  const db = level > 0 ? 20 * Math.log10(level) : -60;
  const pct = Math.max(0, Math.min(100, ((db + 48) / 48) * 100));
  const color = level >= 0.99 ? "bg-red-500" : db > -6 ? "bg-amber-500" : "bg-emerald-500";
  return (
    <div className="flex items-center gap-2" title="Livello del microfono: parla o canta e controlla che arrivi nella zona verde, senza toccare il rosso">
      <Mic className={`h-4 w-4 ${live ? "text-zinc-500" : "text-zinc-300 dark:text-zinc-700"}`} />
      <div className="relative h-2.5 flex-1 overflow-hidden rounded-full bg-zinc-200 dark:bg-zinc-800">
        <div className={`h-full ${color} transition-[width] duration-75`} style={{ width: `${pct}%` }} />
        {/* -12 dB: a good peak for singing */}
        <div className="absolute inset-y-0 w-px bg-zinc-400/70" style={{ left: `${((48 - 12) / 48) * 100}%` }} />
      </div>
      <span className="w-14 text-right text-xs tabular-nums text-zinc-500">{level > 0.0005 ? `${db.toFixed(0)} dB` : "—"}</span>
    </div>
  );
}

/** Microphone and headphones: choice, live level, warnings. */
export function MicCheck({
  mic,
  devices,
  input,
  output,
  setInput,
  setOutput,
  issue,
  retry,
  disabled,
  showHint,
}: {
  mic: MicState;
  devices: Devices | null;
  input: string;
  output: string;
  setInput: (v: string) => void;
  setOutput: (v: string) => void;
  issue: string | null;
  retry: () => void;
  disabled: boolean;
  /** How to read the meter, until the first take of the session. */
  showHint: boolean;
}) {
  // the level arrives 20 times a second: keep it here, not in the whole page
  const [level, setLevel] = useState(0);
  useEffect(() => {
    const un = listen<number>("audio-level", (e) => setLevel(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, []);
  const st = mic.state === "ok" ? mic.status : null;
  const inputBt = st?.input?.bluetooth;
  return (
    <div className="card space-y-2">
      <div className="grid items-center gap-2 lg:grid-cols-[1fr_1fr_minmax(12rem,1fr)]">
        <label className="flex min-w-0 items-center gap-2">
          <Mic className="h-4 w-4 shrink-0 text-violet-600" />
          <select className="field" value={input} disabled={disabled} onChange={(e) => setInput(e.target.value)} title="Microfono">
            <option value="">Microfono automatico{st?.input ? ` (${st.input.name})` : ""}</option>
            {devices?.inputs.map((d) => (
              <option key={d.name} value={d.name}>
                {d.name}
                {d.bluetooth ? " · Bluetooth" : ""}
              </option>
            ))}
          </select>
        </label>
        <label className="flex min-w-0 items-center gap-2">
          <Headphones className="h-4 w-4 shrink-0 text-violet-600" />
          <select className="field" value={output} disabled={disabled} onChange={(e) => setOutput(e.target.value)} title="Cuffie">
            <option value="">Cuffie di sistema{st ? ` (${st.output.name})` : ""}</option>
            {devices?.outputs.map((d) => (
              <option key={d.name} value={d.name}>
                {d.name}
              </option>
            ))}
          </select>
        </label>
        {mic.state === "opening" ? (
          <div className="flex items-center gap-2 text-xs text-zinc-500">
            <Loader2 className="h-4 w-4 animate-spin" /> Preparo microfono e cuffie…
          </div>
        ) : (
          <LevelMeter level={mic.state === "ok" ? level : 0} live={mic.state === "ok"} />
        )}
      </div>

      {(mic.state === "error" || issue) && (
        <div className="flex items-start gap-2 rounded-md bg-red-50 p-2 text-xs text-red-800 dark:bg-red-950/40 dark:text-red-200">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="flex-1">{mic.state === "error" ? mic.error : issue}</div>
          <button className="btn shrink-0 py-0.5 text-xs" onClick={retry}>
            <RefreshCw className="h-3.5 w-3.5" /> Riprova
          </button>
        </div>
      )}
      {inputBt && (
        <div className="flex items-start gap-2 rounded-md bg-amber-50 p-2 text-xs text-amber-800 dark:bg-amber-950/40 dark:text-amber-200">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            Stai registrando con il microfono di cuffie Bluetooth ({st?.input?.name}): la qualità è bassa per un cantante e le cuffie
            passano in modalità «telefonata». Scegli il microfono del Mac o un microfono USB.
          </div>
        </div>
      )}
      {st?.input_switched && !inputBt && (
        <div className="text-xs text-zinc-500">
          Il microfono di sistema è quello delle cuffie Bluetooth: registro con «{st.input?.name}», così le cuffie restano in alta qualità.
        </div>
      )}
      {showHint && mic.state === "ok" && !issue && (
        <div className="text-xs text-zinc-500">
          Prova a cantare una nota forte: la barra deve restare nel verde (picco intorno a −12 dB). Se resta ferma, controlla in Impostazioni di
          Sistema → Privacy e sicurezza → Microfono che Muse Sing sia attivo.
        </div>
      )}
    </div>
  );
}
