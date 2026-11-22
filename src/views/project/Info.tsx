import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AudioLines, FileMusic, FolderOpen, HardDrive, Info as InfoIcon, Trash2 } from "lucide-react";
import type { ProjectSizes } from "../../lib/api";

export function fmtBytes(n: number) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(n < 10 * 1024 * 1024 ? 1 : 0).replace(".", ",")} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(1).replace(".", ",")} GB`;
}

/** A part of the score as the project uses it. */
export type InfoPart = { name: string; how: string; detail?: string };
/** Generated voices: matching the current choices, made with other choices, or none yet. */
export type AudioState = "pronte" | "vecchie" | "nessuna";

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap gap-x-4 py-1.5">
      <div className="w-40 shrink-0 text-zinc-500">{label}</div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

/** The «Info» tab: the piece, the parts, what the project file and the cache weigh. */
export function Info({
  heading,
  parts,
  settings,
  path,
  dirty,
  sizes,
  audio,
  keepAudio,
  onKeepAudio,
  onRemoveAudio,
  cacheBytes,
  onClearCache,
  busy,
}: {
  /** file name · metres · tempo · duration */
  heading: string;
  parts: InfoPart[];
  settings: [string, string][];
  path: string | null;
  dirty: boolean;
  sizes: ProjectSizes | null;
  audio: AudioState;
  keepAudio: boolean;
  onKeepAudio: (v: boolean) => void;
  onRemoveAudio: () => void;
  cacheBytes: number | null;
  onClearCache: () => void;
  busy: boolean;
}) {
  const audioText = { pronte: "generate e aggiornate alle scelte attuali", vecchie: "generate con scelte diverse: da rigenerare", nessuna: "non ancora generate" }[audio];
  return (
    <>
      <section className="card">
        <div className="label flex items-center gap-2">
          <FileMusic className="h-3.5 w-3.5" /> Brano
        </div>
        <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
          <Row label="Partitura">{heading}</Row>
          {parts.map((p) => (
            <Row key={p.name} label={p.name}>
              <span className="font-medium">{p.how}</span>
              {p.detail && <span className="text-zinc-500"> · {p.detail}</span>}
            </Row>
          ))}
          {settings.map(([k, v]) => (
            <Row key={k} label={k}>
              {v}
            </Row>
          ))}
        </div>
      </section>

      <section className="card">
        <div className="label flex items-center gap-2">
          <InfoIcon className="h-3.5 w-3.5" /> File del progetto
        </div>
        <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
          <Row label="File">
            {path ? (
              <span className="flex flex-wrap items-center gap-2">
                <span className="break-all select-text">{path}</span>
                <button className="btn" onClick={() => revealItemInDir(path)}>
                  <FolderOpen className="h-4 w-4" /> Mostra nel Finder
                </button>
              </span>
            ) : (
              <span className="text-zinc-500">non ancora salvato</span>
            )}
            {path && dirty && <div className="text-xs text-amber-600">ci sono modifiche non salvate: i numeri qui sotto sono quelli del file</div>}
          </Row>
          {path && sizes && (
            <>
              <Row label="Dimensione">
                <b>{fmtBytes(sizes.total)}</b>
              </Row>
              <Row label="di cui">
                partitura {fmtBytes(sizes.score)} · impostazioni {fmtBytes(sizes.settings)} · audio{" "}
                {sizes.audio > 0 ? `${fmtBytes(sizes.audio)} (${sizes.audio_parts.join(", ")})` : "nessuno"}
              </Row>
            </>
          )}
        </div>
      </section>

      <section className="card space-y-3">
        <div className="label flex items-center gap-2">
          <AudioLines className="h-3.5 w-3.5" /> Audio
        </div>
        <p>
          Voci <b>{audioText}</b>.
        </p>
        <label className="flex items-start gap-2">
          <input type="checkbox" className="mt-0.5 accent-violet-600" checked={keepAudio} onChange={(e) => onKeepAudio(e.target.checked)} />
          <span>
            Salva l'audio nel progetto
            <span className="block text-xs text-zinc-500">
              Le voci generate vanno nel file (circa 2–3 MB per minuto per voce): riaprendolo, anche su un altro Mac, sono subito pronte. Le parti suonate e
              l'accompagnamento si rifanno da soli in pochi secondi.
            </span>
          </span>
        </label>
        {path && sizes && sizes.audio > 0 && (
          <button className="btn" onClick={onRemoveAudio} disabled={busy}>
            <Trash2 className="h-4 w-4" /> Rimuovi l'audio dal progetto ({fmtBytes(sizes.audio)})
          </button>
        )}
      </section>

      <section className="card space-y-2">
        <div className="label flex items-center gap-2">
          <HardDrive className="h-3.5 w-3.5" /> Cache di Muse Sing
        </div>
        <p className="text-zinc-600 dark:text-zinc-400">
          Muse Sing tiene da parte le voci generate di tutti i progetti, frase per frase, per rigenerare in fretta dopo un ritocco (al massimo 2 GB: le più vecchie
          si buttano da sole). Svuotarla libera spazio; i progetti con l'audio salvato restano pronti, gli altri vanno rigenerati.
        </p>
        <div className="flex flex-wrap items-center gap-3">
          <span>
            Occupa <b>{cacheBytes === null ? "…" : fmtBytes(cacheBytes)}</b>
          </span>
          <button className="btn" onClick={onClearCache} disabled={busy || !cacheBytes}>
            <Trash2 className="h-4 w-4" /> Svuota la cache
          </button>
        </div>
      </section>
    </>
  );
}
