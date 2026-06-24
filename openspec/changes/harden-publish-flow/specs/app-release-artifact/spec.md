## ADDED Requirements

### Requirement: Artifact 写入对并发多 target 安全

server 写入 artifact 行 SHALL 是数据库层面的原子 upsert(`INSERT ... ON CONFLICT (release_id, platform, target, arch, abi) DO UPDATE`),不得使用应用层 SELECT-then-INSERT。多个 target 并发写入同一 `(app, version)` release 时,所有 target 的 artifact MUST 全部持久化,任何一个都不得因写-写竞争而静默丢失。

#### Scenario: 多 target 并发上传同一 version 全部留存
- **WHEN** 4 个不同 target(如 aarch64-apple-darwin / x86_64-apple-darwin / x86_64-unknown-linux-gnu / x86_64-pc-windows-msvc)并发对同一 `(app, version)` 完成上传
- **THEN** 该 release 下 MUST 存在 4 个 artifact 行,每个 target 各一,无丢失、无静默覆盖

#### Scenario: 同 target 重传是幂等 upsert
- **WHEN** 同一 `(release, platform, target, arch, abi)` 被重复上传(重跑/补传)
- **THEN** server MUST 更新该行(filename/size/sha256/object_key/签名),而不是新增重复行,且不报冲突错误

### Requirement: artifact 唯一性约束兜底

artifact 表 SHALL 拥有 `(release_id, platform, target, arch, abi)` 的数据库唯一索引,作为并发竞争的最终兜底:即便未来出现新的竞态,结果 MUST 是约束冲突错误而非静默覆盖。

#### Scenario: 唯一约束阻止静默覆盖
- **WHEN** 两个写入尝试以相同 `(release_id, platform, target, arch, abi)` 元组并发到达
- **THEN** 二者经 `ON CONFLICT DO UPDATE` 收敛为同一行的确定性更新,数据库层 MUST 不产生两条重复行

### Requirement: 发布语义与 artifact 上传解耦

release 的「发布」(draft → published)SHALL 不再作为单个 artifact `complete` 的副作用发生,而是由独立的 finalize 操作显式触发(见 storage-and-presign-upload)。artifact 上传 MUST 不改变 release 的发布状态。

#### Scenario: 上传 artifact 不发布 release
- **WHEN** 任一 target 完成 artifact 上传
- **THEN** release 的发布状态 MUST 保持不变(draft 仍为 draft),发布只能由显式 finalize 改变
