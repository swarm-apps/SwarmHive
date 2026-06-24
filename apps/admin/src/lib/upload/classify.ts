// 从文件名推断产物的平台分类 + Tauri .sig 配对。纯函数,可单测——上传 UI 据此
// 预填 platform/target/abi,并把 .sig 与同名 bundle 关联。

import type { Platform } from "@/lib/api/apps";
import type { ArtifactKind } from "@/lib/api/releases";

export interface Classification {
  platform: Platform;
  kind: ArtifactKind;
  target?: string;
  abi?: string;
  /** 未知扩展名:默认 tauri-desktop,但需用户确认。 */
  uncertain: boolean;
}

// Android ABI 子串(顺序很重要:x86_64 必须排在 x86 前,否则会先命中 x86)。
const ANDROID_ABIS = ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"] as const;

// Tauri 桌面 bundle 的扩展名(小写比较)。
const DESKTOP_EXTS = [
  ".msi",
  ".exe",
  ".dmg",
  ".app.tar.gz",
  ".appimage",
  ".appimage.tar.gz",
  ".deb",
  ".rpm",
  ".nsis.zip",
] as const;

/** 是否是 Tauri 签名文件(`.sig`)。 */
export function isSignatureFile(filename: string): boolean {
  return filename.toLowerCase().endsWith(".sig");
}

/** 从文件名推断平台分类。 */
export function classifyArtifact(filename: string): Classification {
  const lower = filename.toLowerCase();
  if (lower.endsWith(".apk")) {
    const abi = ANDROID_ABIS.find((a) => lower.includes(a));
    return { platform: "react-native-android", kind: "universal", abi, uncertain: false };
  }
  if (DESKTOP_EXTS.some((ext) => lower.endsWith(ext))) {
    return { platform: "tauri-desktop", kind: inferTauriKind(lower), uncertain: false };
  }
  return { platform: "tauri-desktop", kind: "universal", uncertain: true };
}

function inferTauriKind(lowerFilename: string): ArtifactKind {
  if (
    lowerFilename.endsWith(".app.tar.gz") ||
    lowerFilename.endsWith(".appimage.tar.gz") ||
    lowerFilename.endsWith(".nsis.zip") ||
    lowerFilename.endsWith(".msi.zip")
  ) {
    return "updater";
  }
  if (
    lowerFilename.endsWith(".dmg") ||
    lowerFilename.endsWith(".deb") ||
    lowerFilename.endsWith(".rpm")
  ) {
    return "installer";
  }
  return "universal";
}

export interface SigPairing {
  /** 非签名的 bundle 文件名。 */
  bundles: string[];
  /** bundle 文件名 → 配对的 `.sig` 文件名(有签名的才在此)。 */
  signatureByBundle: Record<string, string>;
  /** 找不到同名 bundle 的孤立 `.sig`(应报错,不上传)。 */
  orphanSignatures: string[];
}

/**
 * 把 `.sig` 与同名 bundle 配对:`Foo.app.tar.gz.sig` ↔ `Foo.app.tar.gz`。
 * `.sig` 本身不作为产物上传;其文本在 complete 时随对应 bundle part 上送。
 */
export function pairSignatures(filenames: string[]): SigPairing {
  const bundles = filenames.filter((f) => !isSignatureFile(f));
  const bundleSet = new Set(bundles);
  const signatureByBundle: Record<string, string> = {};
  const orphanSignatures: string[] = [];
  for (const sig of filenames.filter(isSignatureFile)) {
    // 去掉末尾 4 个字符(".sig")得到期望的 bundle 名。
    const bundle = sig.slice(0, -4);
    if (bundleSet.has(bundle)) {
      signatureByBundle[bundle] = sig;
    } else {
      orphanSignatures.push(sig);
    }
  }
  return { bundles, signatureByBundle, orphanSignatures };
}
