import { AudioLines, BookOpen, Mic2 } from "lucide-react";
import { useState } from "react";
import Convert from "./views/Convert";
import Manual, { type ManualPage } from "./views/Manual";
import Record from "./views/Record";

type Tab = "convert" | "record" | "manual";

export default function App() {
  const [tab, setTab] = useState<Tab>(() => {
    const t = new URLSearchParams(location.search).get("tab");
    return t === "record" || t === "manual" ? t : "convert";
  });
  const [manualPage, setManualPage] = useState<ManualPage>("manual");
  const tabBtn = (t: Tab, label: string, Icon: typeof Mic2) => (
    <button
      onClick={() => setTab(t)}
      className={`inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 font-medium ${
        tab === t ? "bg-violet-600 text-white" : "text-zinc-600 hover:bg-zinc-200 dark:text-zinc-300 dark:hover:bg-zinc-800"
      }`}
    >
      <Icon className="h-4 w-4" /> {label}
    </button>
  );
  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-2 border-b border-zinc-200 px-5 py-2.5 dark:border-zinc-800">
        <Mic2 className="h-5 w-5 text-violet-600" />
        <h1 className="mr-4 text-base font-semibold">Muse Sing</h1>
        {tabBtn("convert", "Da MuseScore a OpenUtau", Mic2)}
        {tabBtn("record", "Registra un cantante", AudioLines)}
        <div className="ml-auto flex items-center gap-2">
          {tabBtn("manual", "Manuale", BookOpen)}
          <button
            className="rounded px-1.5 py-0.5 text-xs text-zinc-500 hover:bg-zinc-200 hover:text-zinc-800 dark:hover:bg-zinc-800 dark:hover:text-zinc-200"
            title="Novità di questa versione"
            onClick={() => {
              setManualPage("changelog");
              setTab("manual");
            }}
          >
            v{__APP_VERSION__}
          </button>
        </div>
      </header>
      <Convert active={tab === "convert"} />
      <Record active={tab === "record"} />
      <Manual active={tab === "manual"} page={manualPage} setPage={setManualPage} />
    </div>
  );
}
