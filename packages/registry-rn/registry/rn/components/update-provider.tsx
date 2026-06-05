// update-provider —— 异步装配 engine 并通过 context 注入;可选挂载即 check + 回前台重检。
// 镜像 registry-web 的 tauri 版,RN 差异:recheckOnFocus 用 AppState 'change'→'active'
// 替 window 'focus'(RN 无 window/focus 事件),作为 native 安装路径的兜底复核钩子
//   (用户从系统安装确认框 / 未知来源设置页返回时,主动 check + versionCode 复核;
//    见 add-registry-rn/design.md D5)。
// registry:component,registryDependency: @swarmhive-rn/use-update。

import type { UpdateEngine } from "@swarm-hive/sdk";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { AppState, type AppStateStatus } from "react-native";
import {
  type CreateSwarmHiveEngineOptions,
  createSwarmHiveEngine,
  UpdateEngineContext,
} from "@/hooks/use-update";

export interface UpdateProviderProps extends CreateSwarmHiveEngineOptions {
  children: ReactNode;
  /** engine 装配完成前(取版本 + client_id,通常不到一帧)显示的内容,默认 null。 */
  fallback?: ReactNode;
  /** 挂载后自动 check 一次,默认 true。 */
  checkOnMount?: boolean;
  /** App 回前台(AppState→active)时重新 check(走 engine 节流),默认 true。 */
  recheckOnFocus?: boolean;
}

export function UpdateProvider({
  children,
  fallback = null,
  checkOnMount = true,
  recheckOnFocus = true,
  ...engineOpts
}: UpdateProviderProps) {
  const [engine, setEngine] = useState<UpdateEngine | null>(null);
  // engineOpts 仅首次装配用;后续变化不重建 engine(避免丢失下载状态)。
  const optsRef = useRef(engineOpts);

  useEffect(() => {
    let cancelled = false;
    void createSwarmHiveEngine(optsRef.current).then((created) => {
      if (cancelled) return;
      setEngine(created);
      if (checkOnMount) void created.getState().check();
    });
    return () => {
      cancelled = true;
    };
  }, [checkOnMount]);

  useEffect(() => {
    if (!engine || !recheckOnFocus) return;
    // 回前台兜底:engine.check 内部节流,频繁 active 不会重复打 endpoint;
    // 但对 native 安装路径"返回键关确认框无回调"的悬挂态,这是把状态推回
    // force-required / available 继续劝的关键钩子(check 会按 versionCode 复核)。
    const onChange = (state: AppStateStatus) => {
      if (state === "active") void engine.getState().check();
    };
    const sub = AppState.addEventListener("change", onChange);
    return () => sub.remove();
  }, [engine, recheckOnFocus]);

  if (!engine) return <>{fallback}</>;

  return <UpdateEngineContext.Provider value={engine}>{children}</UpdateEngineContext.Provider>;
}
