import defaultMdxComponents from "fumadocs-ui/mdx";
import type { MDXComponents } from "mdx/types";
import { ComponentPreview } from "@/components/component-preview";
import { Mermaid } from "@/components/mermaid";
import { SnackPreview } from "@/components/snack-preview";

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    // web(Tauri)组件 live preview:MDX 里写 <ComponentPreview name="…" />
    ComponentPreview,
    // RN 组件 Expo Snack 预览:MDX 里写 <SnackPreview name="…" />
    SnackPreview,
    // 架构 / 流程 / 状态机示意图:MDX 里写 <Mermaid chart={`...`} />
    Mermaid,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
