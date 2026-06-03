import type { Platform } from "@/lib/api/apps";

// Tauri target triple → 友好名(产物表格"架构"列展示用)。未列出的 triple 回退原值。
const TRIPLE_LABEL: Record<string, string> = {
  "aarch64-apple-darwin": "macOS Apple Silicon",
  "x86_64-apple-darwin": "macOS Intel",
  "universal-apple-darwin": "macOS Universal",
  "x86_64-pc-windows-msvc": "Windows x64",
  "aarch64-pc-windows-msvc": "Windows ARM64",
  "i686-pc-windows-msvc": "Windows x86",
  "x86_64-unknown-linux-gnu": "Linux x64",
  "aarch64-unknown-linux-gnu": "Linux ARM64",
};

/**
 * 把一个 artifact 的架构维度渲染成友好名:
 * - React Native Android 用 `abi` 原值(arm64-v8a 等);
 * - Tauri 桌面把 target triple 映射成「macOS Apple Silicon」之类,未知 triple 回退原值;
 * - 都缺时回退 `—`。
 */
export function friendlyArch(
  platform: Platform,
  target: string | null | undefined,
  abi: string | null | undefined,
): string {
  if (platform === "react-native-android") {
    return abi || target || "—";
  }
  if (target) {
    return TRIPLE_LABEL[target] ?? target;
  }
  return abi || "—";
}

/**
 * 按 platform 合并表格首列的 rowSpan 计算。**输入必须已按 platform 稳定排序**:
 * 返回与输入等长的数组,每个 platform 连续段的首行 = 段长度、其余行 = 0,
 * 配合 AntD ProTable 的 `onCell: (_, i) => ({ rowSpan: spans[i] })` 使用
 * (rowSpan: 0 表示该单元格被上方合并、不渲染)。
 */
export function platformRowSpans(platforms: string[]): number[] {
  const spans = new Array<number>(platforms.length).fill(0);
  let i = 0;
  while (i < platforms.length) {
    let j = i;
    while (j < platforms.length && platforms[j] === platforms[i]) {
      j += 1;
    }
    spans[i] = j - i; // 段首行承载整段的 rowSpan
    i = j;
  }
  return spans;
}
