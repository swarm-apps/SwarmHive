// demo 名册:既给 /preview/[name] 路由的 generateStaticParams,
// 又给 demo-stage 的动态加载表做 key。新增组件参考页时在此追加一项,
// 并在 demo-stage.tsx 登记对应动态 import。

export const DEMO_NAMES = [
  "prompt-update-dialog",
  "force-update-dialog",
  "update-progress-dialog",
  "update-settings-section",
  "release-notes-view",
  "update-provider",
] as const;

export type DemoName = (typeof DEMO_NAMES)[number];
