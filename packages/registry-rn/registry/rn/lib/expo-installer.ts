// expo-installer —— ApkInstaller 的方案 A 真实实现(零原生代码)。
//
// **本 registry 是该组件的上游 source of truth**:SwarmDrop-RN / SwarmNote-RN 及任何新 app
// 都从这里拉取,不各自演化 —— 要改就改这里,再让双端重新拉。需 Android 模拟器/真机验证
// (本仓 vitest 暂未覆盖它)。
//
// 安装链路(全部走 expo 自带能力,无 Kotlin / 无自写 FileProvider):
//   1. getContentUriAsync(apkPath) —— expo-file-system 自带 FileProvider,把 file:// 转
//      成可授权的 content:// URI;
//   2. IntentLauncher.startActivityAsync(ACTION_VIEW, { data: contentUri,
//      type: application/vnd.android.package-archive,
//      flags: GRANT_READ_URI_PERMISSION | ACTIVITY_NEW_TASK }) —— 交给系统 PackageInstaller。
//
// fire-and-forget:intent 派发即 resolve;系统弹「安装新版本?」对话框,用户确认后本进程
// 被替换,取消则 SDK engine 下次 check 再弹(AppState recheck 兜底)。Android 不允许第三方
// 静默安装,APK 真伪由系统安装器验签兜底。
//
// 依赖:expo-file-system(legacy 子入口)、expo-intent-launcher、react-native。
// 需在 app.config 注入 with-android-install-permission config plugin
// (只注 REQUEST_INSTALL_PACKAGES 权限)。

// createDownloadResumable / getContentUriAsync 在 expo-file-system v18+ 仅保留在 legacy
// 命名空间;新的 OOP File API 还没暴露 content:// 帮手,故继续用 legacy 导入。
import * as FileSystem from "expo-file-system/legacy";
import * as IntentLauncher from "expo-intent-launcher";
import { Platform } from "react-native";
import type { ApkInstaller } from "./ports";

/** android.content.Intent#FLAG_GRANT_READ_URI_PERMISSION */
const FLAG_GRANT_READ_URI_PERMISSION = 0x00000001;
/** android.content.Intent#FLAG_ACTIVITY_NEW_TASK */
const FLAG_ACTIVITY_NEW_TASK = 0x10000000;

/** iOS / 非 Android 平台不支持 in-app 安装(由 TestFlight / App Store 接管)。 */
export class ApkInstallNotSupportedOnIosError extends Error {
  constructor() {
    super("In-app APK install is not supported on iOS");
    this.name = "ApkInstallNotSupportedOnIosError";
  }
}

/**
 * 创建方案 A 的 ApkInstaller。install(apkPath):
 *   file:// 路径 → getContentUriAsync → ACTION_VIEW(package-archive)→ 系统 PackageInstaller。
 * intent 派发即 resolve(fire-and-forget);非 Android 抛 ApkInstallNotSupportedOnIosError。
 */
export function createExpoApkInstaller(): ApkInstaller {
  return {
    async install(apkPath: string): Promise<void> {
      if (Platform.OS !== "android") {
        throw new ApkInstallNotSupportedOnIosError();
      }
      // expo-file-system 自带 FileProvider:把本地 file:// 转成可对外授权的 content:// URI。
      const contentUri = await FileSystem.getContentUriAsync(apkPath);
      await IntentLauncher.startActivityAsync("android.intent.action.VIEW", {
        data: contentUri,
        type: "application/vnd.android.package-archive",
        flags: FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK,
      });
      // 不等安装结果:控制权已交给系统对话框(fire-and-forget handoff)。
    },
  };
}
