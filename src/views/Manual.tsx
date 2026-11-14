import { openUrl } from "@tauri-apps/plugin-opener";
import { marked, type Tokens } from "marked";
import { useMemo, useRef, useState, type MouseEvent } from "react";
import changelog from "../../CHANGELOG.md?raw";
import manual from "../../docs/MANUALE.md?raw";

const slug = (s: string) =>
  s
    .toLowerCase()
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

// unique ids ("Nuovo" repeats in every changelog version); reset before each pass over a document
let seen = new Map<string, number>();
const uniq = (text: string) => {
  const base = slug(text);
  const n = seen.get(base) ?? 0;
  seen.set(base, n + 1);
  return n ? `${base}-${n + 1}` : base;
};

marked.use({
  gfm: true,
  renderer: {
    heading(this: { parser: { parseInline: (t: Tokens.Heading["tokens"]) => string } }, t: Tokens.Heading) {
      return `<h${t.depth} id="${uniq(t.text)}">${this.parser.parseInline(t.tokens)}</h${t.depth}>\n`;
    },
  },
});

function render(md: string) {
  seen = new Map();
  const toc = marked
    .lexer(md)
    .filter((t): t is Tokens.Heading => t.type === "heading")
    .map((t) => ({ id: uniq(t.text), text: t.text.replace(/^\d+\.\s*/, ""), depth: t.depth, sub: t.depth === 3 }))
    .filter((t) => t.depth === 2 || t.depth === 3);
  seen = new Map();
  return { html: marked.parse(md, { async: false }) as string, toc };
}

export type ManualPage = "manual" | "changelog";

export default function Manual({ active, page, setPage }: { active: boolean; page: ManualPage; setPage: (p: ManualPage) => void }) {
  const docs = useMemo(() => ({ manual: render(manual), changelog: render(changelog) }), []);
  const [current, setCurrent] = useState<string | null>(null);
  const body = useRef<HTMLDivElement>(null);
  const doc = docs[page];

  const goTo = (id: string) => {
    body.current?.querySelector(`#${CSS.escape(id)}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
    setCurrent(id);
  };

  // links: external ones in the browser, #anchors inside the page
  const onClick = (e: MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    const href = a?.getAttribute("href");
    if (!href) return;
    e.preventDefault();
    if (href.startsWith("#")) goTo(href.slice(1));
    else openUrl(href).catch(() => {});
  };

  const tab = (p: ManualPage, label: string) => (
    <button
      onClick={() => {
        setPage(p);
        setCurrent(null);
        body.current?.scrollTo({ top: 0 });
      }}
      className={`rounded-md px-2.5 py-1 font-medium ${page === p ? "bg-zinc-200 dark:bg-zinc-800" : "text-zinc-500 hover:text-zinc-800 dark:hover:text-zinc-200"}`}
    >
      {label}
    </button>
  );

  return (
    <main className={`min-h-0 flex-1 ${active ? "flex" : "hidden"}`}>
      <aside className="w-60 shrink-0 space-y-3 overflow-auto border-r border-zinc-200 p-3 dark:border-zinc-800">
        <div className="flex gap-1">
          {tab("manual", "Manuale")}
          {tab("changelog", "Novità")}
        </div>
        <nav className="space-y-0.5">
          {doc.toc.map((t) => (
            <button
              key={t.id}
              onClick={() => goTo(t.id)}
              className={`block w-full truncate rounded py-1 pr-2 text-left ${t.sub ? "pl-5 text-xs text-zinc-500" : "pl-2 font-medium"} ${
                current === t.id ? "bg-violet-100 text-violet-800 dark:bg-violet-950/60 dark:text-violet-200" : "hover:bg-zinc-100 dark:hover:bg-zinc-900"
              }`}
            >
              {t.text}
            </button>
          ))}
        </nav>
        <div className="border-t border-zinc-200 pt-3 text-xs text-zinc-500 dark:border-zinc-800">Muse Sing {__APP_VERSION__}</div>
      </aside>
      <div ref={body} className="flex-1 overflow-auto px-8 py-6">
        <article className="manual mx-auto max-w-3xl" onClick={onClick} dangerouslySetInnerHTML={{ __html: doc.html }} />
      </div>
    </main>
  );
}
