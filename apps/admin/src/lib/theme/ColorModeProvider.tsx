import { createContext, type ReactNode, useContext } from "react";
import { type UseColorModeResult, useColorMode } from "./useColorMode";

const ColorModeContext = createContext<UseColorModeResult | null>(null);

export function ColorModeProvider({ children }: { children: ReactNode }) {
  const value = useColorMode();
  return <ColorModeContext.Provider value={value}>{children}</ColorModeContext.Provider>;
}

export function useColorModeContext(): UseColorModeResult {
  const ctx = useContext(ColorModeContext);
  if (!ctx) {
    throw new Error("useColorModeContext must be used within a <ColorModeProvider>");
  }
  return ctx;
}
