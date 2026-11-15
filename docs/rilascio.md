# Rilascio di una nuova versione

1. Aggiorna la versione in `package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json` (stesso numero) e scrivi le novità in `CHANGELOG.md`.
2. Commit, poi tag e push:
   ```bash
   git tag vX.Y.Z
   git push origin main vX.Y.Z
   ```
3. Il workflow **Rilascio** (GitHub Actions) crea il `.dmg` per Apple Silicon (ONNX Runtime, il motore del canto, non esiste già pronto per i Mac Intel) e lo pubblica nella release del tag, insieme all'aggiornamento firmato (`.app.tar.gz`, `.sig`) e a `latest.json`.
4. Le app già installate trovano l'aggiornamento all'avvio (`latest.json` dell'ultima release, escluse le pre-release) e chiedono se installarlo.

## Firma degli aggiornamenti

Gli aggiornamenti sono firmati con una chiave del progetto (non è la firma Apple): l'app accetta solo aggiornamenti firmati con questa chiave.

- Chiave pubblica: in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
- Chiave privata: **mai nel repository**. Sta in `~/.tauri/muse-sing.key` sul computer di sviluppo e nel segreto `TAURI_SIGNING_PRIVATE_KEY` del repository GitHub (Settings → Secrets and variables → Actions), senza password.
- Se la chiave privata va persa, le versioni già installate non potranno più aggiornarsi da sole: conservane una copia sicura (ad esempio in un gestore di password).

## macOS senza firma Apple

L'app non è firmata né notarizzata (servirebbe l'Apple Developer Program). Ha la firma **ad-hoc** (`bundle.macOS.signingIdentity: "-"`), controllata dal workflow con `codesign`. La prima volta che si installa a mano:

```bash
xattr -cr "/Applications/Muse Sing.app"
```

Con la firma ad-hoc il permesso per il microfono vale per quella versione: dopo un aggiornamento macOS lo chiede di nuovo, una volta.

## Build locale

`pnpm release:mac` (`scripts/release-mac.sh`) crea lo stesso `.dmg` sul Mac, in `release/`, senza pubblicarlo.
