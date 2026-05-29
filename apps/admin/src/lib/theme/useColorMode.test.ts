import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useColorMode } from "./useColorMode";

interface MockMediaQueryList {
  matches: boolean;
  media: string;
  listeners: Array<(e: MediaQueryListEvent) => void>;
  addEventListener: ReturnType<typeof vi.fn>;
  removeEventListener: ReturnType<typeof vi.fn>;
  dispatch: (matches: boolean) => void;
}

function installMatchMedia(initialMatches: boolean): MockMediaQueryList {
  const list: MockMediaQueryList = {
    matches: initialMatches,
    media: "(prefers-color-scheme: dark)",
    listeners: [],
    addEventListener: vi.fn((_event: string, handler: (e: MediaQueryListEvent) => void) => {
      list.listeners.push(handler);
    }),
    removeEventListener: vi.fn((_event: string, handler: (e: MediaQueryListEvent) => void) => {
      const idx = list.listeners.indexOf(handler);
      if (idx >= 0) list.listeners.splice(idx, 1);
    }),
    dispatch: (matches: boolean) => {
      list.matches = matches;
      for (const fn of [...list.listeners]) {
        fn({ matches } as MediaQueryListEvent);
      }
    },
  };

  window.matchMedia = vi.fn().mockReturnValue(list);
  return list;
}

describe("useColorMode", () => {
  beforeEach(() => {
    installMatchMedia(false);
  });

  it("defaults to 'system' mode when localStorage is empty", () => {
    const { result } = renderHook(() => useColorMode());
    expect(result.current.mode).toBe("system");
  });

  it("persists setMode('dark') into localStorage and resolves dark", () => {
    const { result } = renderHook(() => useColorMode());

    act(() => {
      result.current.setMode("dark");
    });

    expect(window.localStorage.getItem("swarmhive-color-mode")).toBe("dark");
    expect(result.current.mode).toBe("dark");
    expect(result.current.resolved).toBe("dark");
  });

  it("tracks system preference live when mode === 'system'", () => {
    const mq = installMatchMedia(false);
    const { result } = renderHook(() => useColorMode());

    expect(result.current.resolved).toBe("light");

    act(() => {
      mq.dispatch(true);
    });

    expect(result.current.resolved).toBe("dark");
  });

  it("explicit 'light' overrides system preference", () => {
    const mq = installMatchMedia(true);
    const { result } = renderHook(() => useColorMode());

    expect(result.current.resolved).toBe("dark");

    act(() => {
      result.current.setMode("light");
    });

    expect(result.current.resolved).toBe("light");
    // system change should not affect resolved when explicitly set
    act(() => {
      mq.dispatch(false);
    });
    expect(result.current.resolved).toBe("light");
  });
});
