import { useEffect, useState } from "react";

/** A per-machine preference kept in localStorage (falls back to `init` if storage is unavailable). */
export function useStored<T>(key: string, init: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => {
    try {
      const s = localStorage.getItem(key);
      return s === null ? init : (JSON.parse(s) as T);
    } catch {
      return init;
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify(v));
    } catch {
      /* private mode */
    }
  }, [key, v]);
  return [v, setV];
}
