// update-dialog-visibility —— 更新弹窗的可见性策略(纯函数,无 React / 无 IO)。
//
// 为什么单独成文件:三个弹窗各自 useUpdate() 自管可见性,一旦条件重叠就会同框——而两个
// AlertDialog 同框时,上层那个全屏 overlay 会吞掉下层 release notes 的滚动手势。渲染层测不了
// (本 registry 无 RN render 设施),抽成纯函数才能把「任何 status × 任何 upgradeType 下至多
// 一个弹窗承载进度」这条不变量真正锁进测试。
//
// registry:lib。

import type { ReleaseInfo, UpdateStatus } from "@swarm-hive/sdk";

/**
 * 是否强制升级流。**唯一真相源是 `release.upgradeType`**,不是 `status`。
 *
 * engine 判 forced(`release.upgradeType === "force"`)、从 error 恢复态用的都是它,且它在
 * 整个生命周期稳定;`status` 则会在进入 downloading / ready 后把「从哪来」抹平——据它推导
 * 就会把普通更新的下载误判成强制流。
 */
export function isForcedFlow(release: ReleaseInfo | null | undefined): boolean {
  return release?.upgradeType === "force";
}

/** 下载中 / 待安装 —— 需要向用户呈现进度的两个态。 */
function isBusy(status: UpdateStatus): boolean {
  return status === "downloading" || status === "ready";
}

/**
 * force-update-dialog 是否可见。仅强制流;普通更新走 prompt-update-dialog。
 *
 * 本弹窗不可关、无「稍后」,错弹会把用户锁到下载结束,故强制流判据必须严格。
 */
export function forceDialogVisible(
  status: UpdateStatus,
  release: ReleaseInfo | null | undefined,
): boolean {
  return isForcedFlow(release) && (status === "force-required" || isBusy(status));
}

/**
 * update-progress-dialog 的缺省可见性(`open` prop 未覆盖时)。
 *
 * 强制流下 force-update-dialog 常驻且自带内联进度,本弹窗让位。非强制流下 prompt 由宿主在
 * downloading 时收起(交接),由本弹窗接管进度。
 */
export function progressDialogVisible(
  status: UpdateStatus,
  release: ReleaseInfo | null | undefined,
): boolean {
  return !isForcedFlow(release) && isBusy(status);
}
