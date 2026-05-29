// 后台消费的 RFC 9457 problem `type` URI 常量集中登记。
// 一律按 `isApiError(e) && e.type === ERR_*` 分支，不看 title/detail（i18n 后会变）。
// 取自 server `routes/*` + `error.rs` 的实际 type URI。

/** slug / version / channel 名重复等通用冲突（`ApiError::Conflict`，409）。 */
export const ERR_CONFLICT = "https://swarmhive.dev/errors/conflict";

/** DELETE app 时该 app 名下仍有 release（409）。 */
export const ERR_APP_HAS_RELEASES = "https://swarmhive.dev/errors/app-has-releases";

/** channel rollback 时无历史可回滚（422）。 */
export const ERR_NOTHING_TO_ROLLBACK = "https://swarmhive.dev/errors/nothing-to-rollback";

/** complete 时上传对象的 size / 校验和与计划不符（422）。 */
export const ERR_UPLOAD_CHECKSUM_MISMATCH = "https://swarmhive.dev/errors/upload-checksum-mismatch";
