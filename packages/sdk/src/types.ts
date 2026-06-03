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
  /** 直链下载地址。 */
  url: string;
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
