// At startup: if GitHub has a newer release, offer to install it (signed with the project's
// updater key, see docs/rilascio.md). Silent when offline or in development.
import { ask } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

/** `onStatus`: a short text while downloading, null when done or failed. */
export async function checkForUpdate(onStatus: (s: string | null) => void) {
  if (import.meta.env.DEV) return;
  try {
    const u = await check();
    if (!u) return;
    const ok = await ask(
      `È disponibile Muse Sing ${u.version} (hai la ${u.currentVersion}).` +
        (u.body ? `\n\n${u.body.slice(0, 600)}` : "") +
        "\n\nAggiornare ora? L'app si riavvierà.",
      { title: "Aggiornamento disponibile", okLabel: "Aggiorna", cancelLabel: "Più tardi" },
    );
    if (!ok) return;
    onStatus("Scarico l'aggiornamento…");
    await u.downloadAndInstall();
    await relaunch();
  } catch (e) {
    console.warn("aggiornamenti:", e);
    onStatus(null);
  }
}

const SEEN = "musesing:news-seen";

/** The changes of this version not looked at yet (a dot next to the version). */
export function hasNews(): boolean {
  try {
    return localStorage.getItem(SEEN) !== __APP_VERSION__;
  } catch {
    return false;
  }
}

export function markNewsSeen() {
  try {
    localStorage.setItem(SEEN, __APP_VERSION__);
  } catch {
    /* not remembered */
  }
}
