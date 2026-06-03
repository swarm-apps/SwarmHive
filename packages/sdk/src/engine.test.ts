import { describe, expect, it } from "vitest";
import { createUpdateEngine } from "./engine";
import type { DownloadHandle, UpdateAdapter } from "./ports";
import { memStorage, mockRelease } from "./test-utils";

function mockAdapter(over: Partial<UpdateAdapter> = {}): UpdateAdapter {
  return {
    check: async () => mockRelease(),
    download: async (rel) => ({ release: rel }) as DownloadHandle,
    install: async () => {},
    storage: memStorage(),
    compare: () => true,
    ...over,
  };
}

describe("createUpdateEngine", () => {
  it("idle → checking → available", async () => {
    const e = createUpdateEngine(mockAdapter(), { currentVersion: "0.4.0", clientId: "c" });
    expect(e.getState().status).toBe("idle");
    await e.getState().check();
    expect(e.getState().status).toBe("available");
    expect(e.getState().release?.version).toBe("0.4.5");
  });

  it("forced update → force-required", async () => {
    const e = createUpdateEngine(
      mockAdapter({ check: async () => mockRelease({ upgradeType: "force" }) }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    expect(e.getState().status).toBe("force-required");
  });

  it("no newer → up-to-date", async () => {
    const e = createUpdateEngine(mockAdapter({ compare: () => false }), {
      currentVersion: "0.4.5",
      clientId: "c",
    });
    await e.getState().check();
    expect(e.getState().status).toBe("up-to-date");
  });

  it("download error is retryable back to checking", async () => {
    let fail = true;
    const e = createUpdateEngine(
      mockAdapter({
        download: async () => {
          if (fail) throw new Error("net");
          return { release: mockRelease() };
        },
      }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    await e.getState().download();
    expect(e.getState().status).toBe("error");
    expect(e.getState().error?.phase).toBe("download");
    fail = false;
    await e.getState().retry();
    expect(e.getState().status).toBe("available");
  });

  it("download → ready → install", async () => {
    let installed = false;
    const e = createUpdateEngine(
      mockAdapter({
        install: async () => {
          installed = true;
        },
      }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    await e.getState().download();
    expect(e.getState().status).toBe("ready");
    await e.getState().install();
    expect(installed).toBe(true);
  });

  it("postpone dismisses available; force bypasses dismiss", async () => {
    const storage = memStorage();
    const e = createUpdateEngine(mockAdapter({ storage }), {
      currentVersion: "0.4.0",
      clientId: "c",
      recheckIntervalMs: 0,
    });
    await e.getState().check();
    expect(e.getState().status).toBe("available");
    await e.getState().postpone();
    await e.getState().check(true);
    expect(e.getState().status).toBe("up-to-date"); // dismissed within TTL

    const e2 = createUpdateEngine(
      mockAdapter({ storage, check: async () => mockRelease({ upgradeType: "force" }) }),
      { currentVersion: "0.4.0", clientId: "c", recheckIntervalMs: 0 },
    );
    await e2.getState().check(true);
    expect(e2.getState().status).toBe("force-required"); // force ignores dismiss
  });

  it("recheck throttle skips within interval, force bypasses", async () => {
    let calls = 0;
    const e = createUpdateEngine(
      mockAdapter({
        check: async () => {
          calls++;
          return mockRelease();
        },
      }),
      { currentVersion: "0.4.0", clientId: "c", recheckIntervalMs: 100_000 },
    );
    await e.getState().check();
    await e.getState().check(); // throttled
    expect(calls).toBe(1);
    await e.getState().check(true); // force bypasses throttle
    expect(calls).toBe(2);
  });
});
