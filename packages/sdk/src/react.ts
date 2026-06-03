// React 订阅层 —— 把 engine 的 vanilla store 订阅成 React state。
// `react` 是 optional peer:只有 import "@swarm-hive/sdk/react" 才需要它,core 不依赖。

import { useStore } from "zustand";
import type { UpdateEngine, UpdateEngineState } from "./engine";

// 模块级默认 selector(引用稳定):内联箭头每次渲染都是新引用,会破坏 zustand 的
// selector 记忆化 → 多余重订阅/重渲染。
const identitySelector = (s: UpdateEngineState): UpdateEngineState => s;

/**
 * 订阅 engine 状态。不传 selector 返回整个 state;传 selector 只在选中片段变化时 re-render。
 *
 * ```tsx
 * const { status, release, check, download } = useUpdateEngine(engine);
 * ```
 */
export function useUpdateEngine<T = UpdateEngineState>(
  engine: UpdateEngine,
  selector: (state: UpdateEngineState) => T = identitySelector as (state: UpdateEngineState) => T,
): T {
  return useStore(engine, selector);
}

export type { UpdateEngine, UpdateEngineState } from "./engine";
