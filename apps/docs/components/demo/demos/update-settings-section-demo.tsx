"use client";

// UpdateSettingsSection 的 live demo:内联(非模态),带场景切换,
// 一次展示「有更新 / 已最新 / 出错」三态。

import { useState } from "react";
import { UpdateSettingsSection } from "@/components/update-settings-section";
import { cn } from "@/lib/utils";
import { DemoUpdateProvider } from "../demo-update-provider";
import type { DemoScenario } from "../mock-adapter";

const SCENARIOS: { key: DemoScenario; label: string }[] = [
  { key: "available", label: "有更新" },
  { key: "up-to-date", label: "已最新" },
  { key: "error", label: "出错" },
];

export default function UpdateSettingsSectionDemo() {
  const [scenario, setScenario] = useState<DemoScenario>("available");
  return (
    <div className="w-full max-w-sm space-y-3">
      <div className="flex gap-1">
        {SCENARIOS.map((s) => (
          <button
            key={s.key}
            type="button"
            onClick={() => setScenario(s.key)}
            className={cn(
              "rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
              scenario === s.key
                ? "bg-primary text-primary-foreground"
                : "bg-muted text-muted-foreground hover:text-foreground",
            )}
          >
            {s.label}
          </button>
        ))}
      </div>
      <DemoUpdateProvider key={scenario} scenario={scenario} currentVersion="1.0.0">
        <div className="rounded-lg border p-4">
          <UpdateSettingsSection locale="zh-CN" currentVersion="1.0.0" />
        </div>
      </DemoUpdateProvider>
    </div>
  );
}
