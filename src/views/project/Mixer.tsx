import { SlidersHorizontal, Volume2, VolumeX } from "lucide-react";
import { AMBIENCES, type AccompCfg, type Ambience, type PartCfg } from "../../lib/api";

type MixPatch = Partial<{ volume_db: number; pan: number; mute: boolean; solo: boolean }>;

function panLabel(p: number) {
  if (Math.abs(p) < 0.05) return "C";
  return `${p < 0 ? "S" : "D"} ${Math.round(Math.abs(p) * 100)}`;
}

/** Volume, pan, mute and solo of one track. */
export function MixControls({
  volume,
  pan,
  mute,
  solo,
  onChange,
  disabled = false,
}: {
  volume: number;
  pan: number;
  mute: boolean;
  solo: boolean;
  onChange: (patch: MixPatch) => void;
  disabled?: boolean;
}) {
  return (
    <>
      <label className="flex items-center gap-2">
        <span className="sr-only">Volume</span>
        <input
          type="range"
          min={-24}
          max={6}
          step={1}
          value={volume}
          disabled={disabled}
          onChange={(e) => onChange({ volume_db: Number(e.target.value) })}
          onDoubleClick={() => onChange({ volume_db: 0 })}
          className="w-32 accent-violet-600"
          title="Volume (doppio clic: 0 dB)"
        />
        <span className="w-12 tabular-nums">
          {volume > 0 ? "+" : ""}
          {volume} dB
        </span>
      </label>
      <label className="flex items-center gap-2">
        Pan
        <input
          type="range"
          min={-1}
          max={1}
          step={0.05}
          value={pan}
          disabled={disabled}
          onChange={(e) => onChange({ pan: Number(e.target.value) })}
          onDoubleClick={() => onChange({ pan: 0 })}
          className="w-20 accent-violet-600"
          title="Sinistra / destra (doppio clic: al centro)"
        />
        <span className="w-10 tabular-nums">{panLabel(pan)}</span>
      </label>
      <button
        className={`btn w-24 justify-center text-zinc-700 dark:text-zinc-300 ${mute ? "bg-zinc-200 dark:bg-zinc-700" : ""}`}
        onClick={() => onChange({ mute: !mute })}
        disabled={disabled}
        title="Spegni questa traccia (in ascolto, in Canta e controlla e nei file)"
      >
        {mute ? <VolumeX className="h-3.5 w-3.5" /> : <Volume2 className="h-3.5 w-3.5" />}
        {mute ? "spenta" : "accesa"}
      </button>
      <button
        className={`btn justify-center text-zinc-700 dark:text-zinc-300 ${solo ? "border-amber-500 bg-amber-400 text-amber-950 hover:bg-amber-300 dark:bg-amber-400 dark:text-amber-950" : ""}`}
        onClick={() => onChange({ solo: !solo })}
        disabled={disabled}
        title="Ascolta solo le tracce con «solo» acceso"
      >
        solo
      </button>
    </>
  );
}

function Row({ name, note, children, highlight = false }: { name: string; note?: string; children: React.ReactNode; highlight?: boolean }) {
  return (
    <div className={`flex flex-wrap items-center gap-x-5 gap-y-1 py-2 text-xs text-zinc-500 ${highlight ? "-mx-2 rounded-md bg-violet-500/5 px-2" : ""}`}>
      <div
        className={`w-32 shrink-0 truncate text-sm ${highlight ? "font-semibold text-violet-700 dark:text-violet-300" : "font-medium text-zinc-800 dark:text-zinc-200"}`}
        title={name}
      >
        {name}
        {note && <div className="truncate text-xs font-normal text-zinc-500">{note}</div>}
      </div>
      {children}
    </div>
  );
}

/**
 * The mix of the generated tracks: what you hear, what plays under «Canta e controlla» and what
 * goes into the exported files. Nothing here needs a new render.
 */
export function Mixer({
  parts,
  mine,
  accompaniment,
  metronome,
  ambience,
  reverb,
  onPart,
  onAccompaniment,
  onMetronome,
  onAmbience,
  onReverb,
  disabled,
}: {
  /** the generated voices */
  parts: PartCfg[];
  mine: string | null;
  /** null: no accompaniment generated */
  accompaniment: AccompCfg | null;
  metronome: AccompCfg;
  ambience: Ambience;
  reverb: number;
  onPart: (name: string, patch: MixPatch) => void;
  onAccompaniment: (patch: MixPatch) => void;
  onMetronome: (patch: MixPatch & { enabled?: boolean }) => void;
  onAmbience: (a: Ambience) => void;
  onReverb: (r: number) => void;
  /** while singing the music is already mixed */
  disabled: boolean;
}) {
  return (
    <section className="card">
      <div className="label flex items-center gap-2">
        <SlidersHorizontal className="h-3.5 w-3.5" /> Mixer
        <span className="font-normal normal-case text-zinc-500">
          · vale per l'ascolto, per Canta e controlla e per i file esportati{disabled ? " (bloccato mentre canti)" : ""}
        </span>
      </div>
      <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
        {parts.map((p) => (
          <Row key={p.name} name={p.name} note={p.name === mine ? "la tua voce" : p.played ? "suonata" : undefined} highlight={p.name === mine}>
            <MixControls volume={p.volume_db} pan={p.pan} mute={p.mute} solo={p.solo} onChange={(patch) => onPart(p.name, patch)} disabled={disabled} />
          </Row>
        ))}
        {accompaniment && (
          <Row name="Accompagnamento">
            <MixControls
              volume={accompaniment.volume_db}
              pan={accompaniment.pan}
              mute={accompaniment.mute}
              solo={accompaniment.solo}
              onChange={onAccompaniment}
              disabled={disabled}
            />
          </Row>
        )}
        <Row name="Metronomo">
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              className="accent-violet-600"
              checked={metronome.enabled}
              disabled={disabled}
              onChange={() => onMetronome({ enabled: !metronome.enabled })}
              aria-label="Metronomo"
            />
            {metronome.enabled ? "acceso" : "spento: un colpo per battito, più forte sul primo della battuta"}
          </label>
          {metronome.enabled && (
            <MixControls
              volume={metronome.volume_db}
              pan={metronome.pan}
              mute={metronome.mute}
              solo={metronome.solo}
              onChange={onMetronome}
              disabled={disabled}
            />
          )}
        </Row>
        <Row name="Ambiente">
          <select
            className="field w-40"
            aria-label="Ambiente"
            title="Il riverbero di una stanza o di una chiesa"
            value={ambience}
            disabled={disabled}
            onChange={(e) => onAmbience(e.target.value as Ambience)}
          >
            {Object.entries(AMBIENCES).map(([k, v]) => (
              <option key={k} value={k}>
                {v}
              </option>
            ))}
          </select>
          {ambience !== "nessuno" && (
            <label className="flex items-center gap-2">
              Riverbero
              <input
                type="range"
                min={0}
                max={100}
                step={5}
                value={Math.round(reverb * 100)}
                disabled={disabled}
                onChange={(e) => onReverb(Number(e.target.value) / 100)}
                onDoubleClick={() => onReverb(0.5)}
                className="w-32 accent-violet-600"
                aria-label="Quantità di riverbero"
                title="Doppio clic: valore normale"
              />
              <span className="w-9 tabular-nums">{Math.round(reverb * 100)}%</span>
            </label>
          )}
        </Row>
      </div>
    </section>
  );
}
