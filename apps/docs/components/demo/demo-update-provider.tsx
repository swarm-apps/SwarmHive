"use client";

// demo-update-provider —— 文档站 live preview 专用的 Provider。
// 用 mock adapter 装配一个真实的 SDK engine,注入 registry 组件消费的同一个
// UpdateEngineContext(来自 @/hooks/use-update),于是 registry 的真实组件源码
// 可在浏览器里被原样驱动出各状态。
//
// 与真实 <UpdateProvider> 的关键差异:
//   - 真实 Provider 走 createSwarmHiveEngine → tauriAdapter + getVersion()(依赖 __TAURI__);
//   - 本 Provider 走 createUpdateEngine(mockAdapter) 同步装配,绝不 import @tauri-apps/*,
//     因此能在纯浏览器 / 静态导出环境运行。

import { createUpdateEngine } from "@swarm-hive/sdk";
import { type ReactNode, useEffect, useMemo } from "react";
import { UpdateEngineContext } from "@/hooks/use-update";
import { createMockAdapter, type DemoScenario } from "./mock-adapter";

export interface DemoUpdateProviderProps {
  /** 预览场景:决定 check() 落到 available / force-required / up-to-date / error。 */
  scenario: DemoScenario;
  children: ReactNode;
  /** demo 里的「当前版本」,用于弹窗描述文案。 */
  currentVersion?: string;
  /** 挂载后自动 check 一次(force,绕过节流),默认 true。 */
  checkOnMount?: boolean;
}

export function DemoUpdateProvider({
  scenario,
  children,
  currentVersion = "1.0.0",
  checkOnMount = true,
}: DemoUpdateProviderProps) {
  // scenario / 版本变化时重建 engine(连带全新内存 storage,dismiss 状态清零);
  // recheckIntervalMs:0 关掉 12h 节流,演示可反复触发 check。
  const engine = useMemo(
    () =>
      createUpdateEngine(createMockAdapter(scenario), {
        currentVersion,
        clientId: "demo-client",
        recheckIntervalMs: 0,
      }),
    [scenario, currentVersion],
  );

  useEffect(() => {
    if (checkOnMount) void engine.getState().check(true);
  }, [engine, checkOnMount]);

  return <UpdateEngineContext.Provider value={engine}>{children}</UpdateEngineContext.Provider>;
}
