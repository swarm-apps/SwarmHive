## ADDED Requirements

### Requirement: complete 回调只写 artifact 不发布

presign 直传的 `complete` 回调 SHALL 只负责校验对象、写入 artifact 行、标记 upload session 完成。它 MUST NOT 触发 release 的发布(mark_published)。多 target 并发 `complete` 因此不再互相抢「发布」这一终态副作用。

#### Scenario: complete 后 release 保持 draft
- **WHEN** 客户端对某 target 调用 `complete`
- **THEN** artifact 行写入成功、session 标记完成,且该 release 的状态 MUST 仍为 draft(除非随后显式 finalize)

### Requirement: 显式幂等的 release finalize 端点

server SHALL 提供 `POST /api/v1/apps/{slug}/releases/{version}/finalize` 把 draft release 发布为 published。该端点 MUST 幂等:对已 published 的 release 重复调用 SHALL 返回 200 且不产生副作用。finalize MUST 校验该 release 至少有一个 artifact,否则拒绝。

#### Scenario: finalize 把 draft 发布为 published
- **WHEN** 一个含 ≥1 个 artifact 的 draft release 被 finalize
- **THEN** release 状态变为 published 并记录 published_at,响应返回更新后的 release

#### Scenario: finalize 幂等
- **WHEN** 对一个已 published 的 release 再次 finalize
- **THEN** server MUST 返回 200 且 release 状态/published_at 不变,不报错

#### Scenario: 无 artifact 不可 finalize
- **WHEN** 对一个没有任何 artifact 的 release 调用 finalize
- **THEN** server MUST 拒绝(校验错误),不改变发布状态

### Requirement: 403 Forbidden 携带可执行补救提示

当发布相关操作因缺少权限被拒时,server 返回的 RFC 9457 problem+json SHALL 包含 `required_permission` 字段,并 SHALL 附带可执行的补救提示(remediation hint),指向「如何获得该权限」(例如重建带 `ci-publish` 预设的 token)。

#### Scenario: 缺 release:update 的 403 含补救提示
- **WHEN** 一个缺少 `release:update` 权限的 token 触发需要该权限的操作(如更新已存在 release 的 notes)
- **THEN** 响应 MUST 是 403,problem body 含 `required_permission = "release:update"` 与一行可执行补救提示(如 `swarmhive tokens create --kind api --preset ci-publish`)
