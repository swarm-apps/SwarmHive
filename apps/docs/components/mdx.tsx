import defaultMdxComponents from "fumadocs-ui/mdx";
import type { MDXComponents } from "mdx/types";
import { ComponentPreview } from "@/components/component-preview";
import { SnackPreview } from "@/components/snack-preview";

export function getMDXComponents(components?: MDXComponents) {
  return {
    ...defaultMdxComponents,
    // web(Tauri)组件 live preview:MDX 里写 <ComponentPreview name="…" />
    ComponentPreview,
    // RN 组件 Expo Snack 预览:MDX 里写 <SnackPreview name="…" />
    SnackPreview,
    ...components,
  } satisfies MDXComponents;
}

export const useMDXComponents = getMDXComponents;

declare global {
  type MDXProvidedComponents = ReturnType<typeof getMDXComponents>;
}
