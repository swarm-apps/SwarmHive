// 公共类型 —— SDK 的对外契约,独立于 wire schema(generated/schema.ts)。
// wire→SDK 的归一化在 check-update.ts 完成。

/** 升级强制度。`force` = 客户端必须升级;`silent` = 静默(roadmap,客户端可忽略)。 */
export type UpgradeType = "prompt" | "force" | "silent";

/** 8 态更新状态(与 4 个真实 app + server 收敛一致)。 */
export type UpdateStatus =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "force-required"
  | "downloading"
  | "ready"
  | "error";

/** 归一化的跨平台 release 信息(Tauri `version` / RN `versionCode` 合一)。 */
export interface ReleaseInfo {
  /** 显示版本号(semver,Tauri 主用;RN 也带 versionName)。 */
  version: string;
  /** Android versionCode(整数,RN 主用);Tauri 为 undefined。 */
  versionCode?: number;
  /** 主下载地址(SwarmHive `/download` 间接入口)。 */
  url: string;
  /**
   * 备用下载源候选(当前即 GitHub Release,已过服务端 liveness/digest 校验),按序尝试。
   * 主源失败(错误页 / sha256 不符)时逐个 fallback(`add-github-release-source`)。
   */
  mirrorUrls?: string[];
  /**
   * 期望字节数(RN wire 的 `size_bytes`);Tauri 为 undefined。
   * 下载器用它拦截截断下载 —— 残缺文件的 ZIP magic 仍然合法,只有尺寸能发现。
   */
  sizeBytes?: number;
  /** 完整性/签名:Tauri 的 minisign `.sig` 全文 / RN 的 sha256。 */
  signature?: string;
  /** release notes(markdown 或纯文本,由 UI 层决定渲染方式)。 */
  notes?: string;
  /** 发布时间(RFC 3339)。 */
  pubDate?: string;
  upgradeType: UpgradeType;
  /** 强制更新下限(semver / versionCode 字符串)。 */
  minVersion?: string;
  /** 灰度放量百分比(1-100);undefined = 全量。 */
  rolloutPercent?: number;
  /** 命中的 channel 名。 */
  channel: string;
  /**
   * 轻 OTA 接缝:更新载体类型。缺省 ⇒ `native-package`(整包 APK / Tauri bundle);
   * `ota-bundle`(JS bundle 热更)留给 Phase 2 `add-ota-provider`,MVP 无任何路径产出。
   * 消费方只需判 `release.kind === "ota-bundle"`。
   */
  kind?: "native-package" | "ota-bundle";
}

/** 下载进度。 */
export interface Progress {
  downloaded: number;
  total: number;
  /** 0~1。 */
  percent: number;
  /** 瞬时速度 bytes/s(可选,平台 adapter 可不提供)。 */
  speed?: number;
}

/** 更新流程错误。`phase` 区分检查 vs 下载/安装(对齐业界事件模型 checkError/downloadError)。 */
export class UpdateError extends Error {
  constructor(
    message: string,
    public readonly phase: "check" | "download" | "install",
    public readonly cause?: unknown,
  ) {
    super(message);
    this.name = "UpdateError";
  }
}
