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

  it("install is idempotent and does not consume ready", async () => {
    const handles: DownloadHandle[] = [];
    const e = createUpdateEngine(
      mockAdapter({
        install: async (h) => {
          handles.push(h);
        },
      }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    await e.getState().download();
    await e.getState().install();
    await e.getState().install();
    await e.getState().install();
    // 平台移交可以静默失败(Android 后台派发的安装 intent 被系统丢弃),所以同一个句柄
    // 必须能反复移交 —— 否则那次失败就永久化了,只剩重新下载一条路。
    expect(handles).toHaveLength(3);
    expect(handles[0]).toBe(handles[1]);
    expect(handles[1]).toBe(handles[2]);
    expect(e.getState().status).toBe("ready");
  });

  it("a blocked install stays in ready and surfaces the reason, without an error", async () => {
    const e = createUpdateEngine(mockAdapter({ install: async () => ({ reason: "background" }) }), {
      currentVersion: "0.4.0",
      clientId: "c",
    });
    await e.getState().check();
    await e.getState().download();
    await e.getState().install();

    // 前置条件挡下 ≠ 安装失败:什么都没发生过,产物完好。走 error 会广播一个假故障,
    // 逼每个订阅者去认识平台的错误类型再把状态推回来。
    expect(e.getState().status).toBe("ready");
    expect(e.getState().error).toBeNull();
    expect(e.getState().installBlocked).toBe("background");
  });

  it("a subsequent successful install clears the blocked reason", async () => {
    let blocked = true;
    const e = createUpdateEngine(
      mockAdapter({ install: async () => (blocked ? { reason: "background" } : undefined) }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    await e.getState().download();
    await e.getState().install();
    expect(e.getState().installBlocked).toBe("background");

    blocked = false;
    await e.getState().install();
    expect(e.getState().installBlocked).toBeNull();
    expect(e.getState().status).toBe("ready");
  });

  it("retry clears the blocked reason", async () => {
    const e = createUpdateEngine(mockAdapter({ install: async () => ({ reason: "background" }) }), {
      currentVersion: "0.4.0",
      clientId: "c",
      recheckIntervalMs: 0,
    });
    await e.getState().check();
    await e.getState().download();
    await e.getState().install();
    await e.getState().retry();
    expect(e.getState().installBlocked).toBeNull();
  });

  it("install failure keeps the artifact; acknowledgeError returns to ready", async () => {
    let fail = true;
    const e = createUpdateEngine(
      mockAdapter({
        install: async () => {
          if (fail) throw new Error("background activity launch blocked");
        },
      }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    await e.getState().download();
    await e.getState().install();
    expect(e.getState().status).toBe("error");
    expect(e.getState().error?.phase).toBe("install");

    e.getState().acknowledgeError();
    // 产物还在磁盘上,恢复目标就该是 ready —— 掉回 available 会让 install() 永远够不着。
    expect(e.getState().status).toBe("ready");
    fail = false;
    await e.getState().install();
    expect(e.getState().status).toBe("ready");
  });

  it("acknowledgeError without an artifact falls back to release-derived status", async () => {
    const e = createUpdateEngine(
      mockAdapter({ check: async () => mockRelease({ upgradeType: "force" }) }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    e.setState({ status: "error" });
    e.getState().acknowledgeError();
    expect(e.getState().status).toBe("force-required");
  });

  it("reconcile hit skips download and lands on ready", async () => {
    let downloads = 0;
    const restored = { release: mockRelease() } as DownloadHandle;
    const e = createUpdateEngine(
      mockAdapter({
        download: async (rel) => {
          downloads++;
          return { release: rel };
        },
        reconcile: async (rel) => (rel ? restored : null),
      }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    expect(e.getState().status).toBe("ready");
    expect(downloads).toBe(0);

    let installedWith: DownloadHandle | null = null;
    e.setState({ status: "ready" });
    const e2 = createUpdateEngine(
      mockAdapter({
        reconcile: async (rel) => (rel ? restored : null),
        install: async (h) => {
          installedWith = h;
        },
      }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e2.getState().check();
    await e2.getState().install();
    expect(installedWith).toBe(restored);
  });

  it("reconcile miss falls back to available", async () => {
    const e = createUpdateEngine(mockAdapter({ reconcile: async () => null }), {
      currentVersion: "0.4.0",
      clientId: "c",
    });
    await e.getState().check();
    expect(e.getState().status).toBe("available");
  });

  it("reconcile throwing degrades to a normal download path", async () => {
    const e = createUpdateEngine(
      mockAdapter({
        reconcile: async () => {
          throw new Error("fs blew up");
        },
      }),
      { currentVersion: "0.4.0", clientId: "c" },
    );
    await e.getState().check();
    expect(e.getState().status).toBe("available");
    expect(e.getState().error).toBeNull();
  });

  it("reconcile(null) cleans up once the version is installed", async () => {
    const calls: (string | null)[] = [];
    const e = createUpdateEngine(
      mockAdapter({
        compare: () => false, // 已是最新
        reconcile: async (rel) => {
          calls.push(rel?.version ?? null);
          return null;
        },
      }),
      { currentVersion: "0.4.5", clientId: "c" },
    );
    await e.getState().check();
    expect(e.getState().status).toBe("up-to-date");
    expect(calls).toEqual([null]);
  });

  it("a dismissed release keeps its artifact instead of cleaning it up", async () => {
    const storage = memStorage();
    const calls: (string | null)[] = [];
    const e = createUpdateEngine(
      mockAdapter({
        storage,
        reconcile: async (rel) => {
          calls.push(rel?.version ?? null);
          return null;
        },
      }),
      { currentVersion: "0.4.0", clientId: "c", recheckIntervalMs: 0 },
    );
    await e.getState().check();
    await e.getState().postpone();
    calls.length = 0;
    await e.getState().check(true);
    expect(e.getState().status).toBe("up-to-date");
    // 「稍后」只是不打扰,不该把已下好的字节丢掉 —— TTL 过期后要能直接命中。
    expect(calls).toEqual([]);
  });

  it("retry discards the artifact", async () => {
    let installs = 0;
    const e = createUpdateEngine(
      mockAdapter({
        install: async () => {
          installs++;
        },
      }),
      { currentVersion: "0.4.0", clientId: "c", recheckIntervalMs: 0 },
    );
    await e.getState().check();
    await e.getState().download();
    await e.getState().retry();
    expect(e.getState().status).toBe("available");
    e.setState({ status: "ready" }); // 强行摆到 ready,验证句柄确实没了
    await e.getState().install();
    expect(installs).toBe(0);
  });

  it("an adapter without reconcile behaves exactly as before", async () => {
    const e = createUpdateEngine(mockAdapter(), { currentVersion: "0.4.0", clientId: "c" });
    await e.getState().check();
    expect(e.getState().status).toBe("available");
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
