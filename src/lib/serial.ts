/** Open/close of the audio devices must not overtake each other (Registra and Canta e controlla). */
let audioChain: Promise<unknown> = Promise.resolve();
export function serial<T>(f: () => Promise<T>): Promise<T> {
  const p = audioChain.then(f, f);
  audioChain = p.catch(() => {});
  return p;
}
