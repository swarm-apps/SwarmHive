import type { ReleaseInfo, UpdateStatus, UpgradeType } from "@swarm-hive/sdk";
import { describe, expect, it } from "vitest";
import {
  forceDialogVisible,
  isForcedFlow,
  progressDialogVisible,
  readyHintKind,
  updateActionKind,
} from "../registry/rn/lib/update-dialog-visibility";

const ALL_STATUSES: UpdateStatus[] = [
  "idle",
  "checking",
  "up-to-date",
  "available",
  "force-required",
  "downloading",
  "ready",
  "error",
];

const ALL_UPGRADE_TYPES: UpgradeType[] = ["prompt", "force", "silent"];

const release = (upgradeType: UpgradeType): ReleaseInfo => ({
  version: "0.7.17",
  url: "https://example.test/download/swarmdrop/0.7.17/apk",
  upgradeType,
  channel: "stable",
});

describe("isForcedFlow", () => {
  it("reads upgradeType, not status", () => {
    expect(isForcedFlow(release("force"))).toBe(true);
    expect(isForcedFlow(release("prompt"))).toBe(false);
    expect(isForcedFlow(release("silent"))).toBe(false);
    expect(isForcedFlow(null)).toBe(false);
    expect(isForcedFlow(undefined)).toBe(false);
  });
});

describe("dialog visibility invariant", () => {
  // 核心不变量:任何 status × 任何 upgradeType 下,承载进度的弹窗至多一个。两个 AlertDialog
  // 同框时上层的全屏 overlay 会吞掉下层的滚动手势——这条不变量就是防它。
  it("never shows force and progress dialogs at the same time", () => {
    for (const status of ALL_STATUSES) {
      for (const upgradeType of ALL_UPGRADE_TYPES) {
        const r = release(upgradeType);
        const both = forceDialogVisible(status, r) && progressDialogVisible(status, r);
        expect(both, `both visible at status=${status} upgradeType=${upgradeType}`).toBe(false);
      }
    }
  });

  it("shows exactly one progress carrier whenever busy", () => {
    for (const status of ["downloading", "ready"] as const) {
      for (const upgradeType of ALL_UPGRADE_TYPES) {
        const r = release(upgradeType);
        const carriers = [forceDialogVisible(status, r), progressDialogVisible(status, r)].filter(
          Boolean,
        ).length;
        expect(carriers, `status=${status} upgradeType=${upgradeType}`).toBe(1);
      }
    }
  });
});

describe("forceDialogVisible", () => {
  // 线上回归(v0.7.16 → 0.7.17,prompt 类型):曾据 status 判断,downloading 一到就弹出这个
  // 不可关的强制弹窗,冒充「需要更新」把用户锁到下载结束。
  it("stays hidden for a non-forced download", () => {
    const r = release("prompt");
    expect(forceDialogVisible("downloading", r)).toBe(false);
    expect(forceDialogVisible("ready", r)).toBe(false);
  });

  it("stays visible across the whole forced flow", () => {
    const r = release("force");
    expect(forceDialogVisible("force-required", r)).toBe(true);
    expect(forceDialogVisible("downloading", r)).toBe(true);
    expect(forceDialogVisible("ready", r)).toBe(true);
  });

  it("hides outside the forced flow's own statuses", () => {
    const r = release("force");
    for (const status of ["idle", "checking", "up-to-date", "available", "error"] as const) {
      expect(forceDialogVisible(status, r), `status=${status}`).toBe(false);
    }
  });

  it("never shows without a release", () => {
    for (const status of ALL_STATUSES) {
      expect(forceDialogVisible(status, null), `status=${status}`).toBe(false);
    }
  });
});

describe("progressDialogVisible", () => {
  it("carries progress for a non-forced download", () => {
    const r = release("prompt");
    expect(progressDialogVisible("downloading", r)).toBe(true);
    expect(progressDialogVisible("ready", r)).toBe(true);
  });

  it("yields to the forced dialog's inline progress", () => {
    const r = release("force");
    expect(progressDialogVisible("downloading", r)).toBe(false);
    expect(progressDialogVisible("ready", r)).toBe(false);
  });

  it("hides when not busy", () => {
    const r = release("prompt");
    for (const status of ["idle", "checking", "up-to-date", "available", "error"] as const) {
      expect(progressDialogVisible(status, r), `status=${status}`).toBe(false);
    }
  });
});

describe("updateActionKind", () => {
  it("maps every status to exactly one action", () => {
    const expected: Record<UpdateStatus, ReturnType<typeof updateActionKind>> = {
      idle: "check",
      checking: "checking",
      "up-to-date": "check",
      available: "download",
      "force-required": "download",
      downloading: "downloading",
      ready: "install",
      error: "check",
    };
    for (const status of ALL_STATUSES) {
      expect(updateActionKind(status), `status=${status}`).toBe(expected[status]);
    }
  });

  it("never lets ready fall through to the check branch", () => {
    // ready 落进「检查更新 / 已是最新」正是 SwarmDrop v0.12.3 的现场:设置页说已是最新,
    // 前面却压着一个说有新版本要装的弹窗。
    expect(updateActionKind("ready")).toBe("install");
  });
});

describe("readyHintKind", () => {
  it("prefers the concrete block reason over the generic hint", () => {
    expect(readyHintKind("background", false)).toBe("background");
    // 门禁原因优先于「自动尝试已用掉」—— 它更具体,也直接指出了用户该做什么。
    expect(readyHintKind("background", true)).toBe("background");
  });

  it("reads a spent auto-attempt with no block as a user cancellation", () => {
    // 没被门禁拦、intent 发出去了、却还停在 ready —— 只能是用户在系统框点了取消。
    expect(readyHintKind(null, true)).toBe("canceled");
  });

  it("defaults to the plain ready hint", () => {
    expect(readyHintKind(null, false)).toBe("ready");
  });
});
