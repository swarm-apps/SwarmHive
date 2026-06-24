## ADDED Requirements

### Requirement: publish 的 notes 更新条件化且后置

`swarmhive publish` 对已存在 release 的 release notes 更新(PATCH)SHALL 仅在 notes 内容实际发生变化时执行;notes 未变化时 MUST 跳过 PATCH,从而不触发 `release:update` 权限检查。notes 的 PATCH SHALL 发生在 artifact 上传之后,使「上传 artifact」这条关键路径不被「改 notes」的权限检查阻塞。CLI SHALL 提供 `--skip-notes-update` 显式跳过 notes 更新。

#### Scenario: 重发且 notes 未变跳过 PATCH
- **WHEN** 对一个已存在 release 重新 publish/补传 target,且传入的 notes 与服务端现有 notes 相同
- **THEN** CLI MUST NOT 发起 notes PATCH,即便 token 缺 `release:update` 也能正常上传 artifact

#### Scenario: notes 变化时才更新且在上传后
- **WHEN** 传入的 notes 与服务端现有 notes 不同
- **THEN** CLI 在 artifact 上传完成之后才 PATCH notes;若 PATCH 因权限失败,已上传的 artifact MUST 不被回滚

### Requirement: publish 默认上传到 draft,发布由 finalize 显式触发

`swarmhive publish` SHALL 默认只上传 artifact 并保持 release 为 draft(不发布)。CLI SHALL 提供 `release finalize` 子命令调用 server 的 finalize 端点完成发布。多 target 发布的推荐流程为:N 个 target 各自 publish(到 draft)→ 最后一次 `release finalize`。

#### Scenario: publish 默认不发布
- **WHEN** 在不带显式发布意图的情况下运行 `swarmhive publish`
- **THEN** artifact 上传成功且 release MUST 保持 draft

#### Scenario: finalize 子命令发布 release
- **WHEN** 所有 target 上传完成后运行 `swarmhive release finalize --app <slug> --version <v>`
- **THEN** CLI 调用 finalize 端点,release 变为 published

### Requirement: CLI 退出码区分永久错误与可重试错误

CLI 进程退出码 SHALL 区分故障类型:永久性错误(401/403/409/422 等权限、配置、冲突)MUST 以 `exit 2` 退出;可重试错误(408/429/5xx、网络/超时)MUST 以 `exit 1` 退出;成功为 `exit 0`。错误响应 SHALL 携带 `retryable` 信号供调用方(如 CI/action)据此决定重试或立即失败。

#### Scenario: 权限错误是永久失败
- **WHEN** 一次操作返回 403(缺权限)
- **THEN** CLI MUST 以 `exit 2` 退出并打印明确的永久错误(含 required_permission)

#### Scenario: 服务端不可用是可重试失败
- **WHEN** 一次操作遇到 5xx 或网络超时
- **THEN** CLI MUST 以 `exit 1` 退出并标注该失败可重试

### Requirement: tokens create 提供 CI 发布预设

`swarmhive tokens create` SHALL 支持 `--preset ci-publish`,一次性展开为 CI 发布所需的完整权限集,且 MUST 包含 `release:update`(本次事故缺失的权限)。用户使用预设时 MUST NOT 需要手工逐项勾选权限。

#### Scenario: ci-publish 预设含完整发布权限
- **WHEN** 运行 `swarmhive tokens create --kind api --preset ci-publish --name <n>`
- **THEN** 创建的 token 权限集 MUST 至少包含 `app:read, release:read, release:create, release:update, release:publish, release:promote, artifact:upload`
