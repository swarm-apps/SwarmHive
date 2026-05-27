import { useCallback, useEffect, useState } from "react";

export type ColorMode = "light" | "dark" | "system";

const STORAGE_KEY = "swarmhive-color-mode";
const DARK_MEDIA_QUERY = "(prefers-color-scheme: dark)";

function readStoredMode(): ColorMode {
  if (typeof window === "undefined") return "system";
  const raw = window.localStorage.getItem(STORAGE_KEY);
  return raw === "light" || raw === "dark" || raw === "system" ? raw : "system";
}

function resolveSystemPreference(): "light" | "dark" {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return "light";
  }
  return window.matchMedia(DARK_MEDIA_QUERY).matches ? "dark" : "light";
}

export interface UseColorModeResult {
  mode: ColorMode;
  resolved: "light" | "dark";
  setMode: (next: ColorMode) => void;
}

export function useColorMode(): UseColorModeResult {
  const [mode, setModeState] = useState<ColorMode>(() => readStoredMode());
  const [systemResolved, setSystemResolved] = useState<"light" | "dark">(() =>
    resolveSystemPreference(),
  );

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const mq = window.matchMedia(DARK_MEDIA_QUERY);
    const handler = (event: MediaQueryListEvent) => {
      setSystemResolved(event.matches ? "dark" : "light");
    };
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  const setMode = useCallback((next: ColorMode) => {
    setModeState(next);
    if (typeof window !== "undefined") {
      window.localStorage.setItem(STORAGE_KEY, next);
    }
  }, []);

  const resolved: "light" | "dark" = mode === "system" ? systemResolved : mode;

  return { mode, resolved, setMode };
}
