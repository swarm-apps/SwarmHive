# app-release-artifact Specification

## Purpose
TBD - created by archiving change add-app-release-artifact. Update Purpose after archive.
## Requirements
### Requirement: Server SHALL manage Apps with auto-seeded channels

The server SHALL expose CRUD for apps under `/api/v1/apps`. `POST /api/v1/apps` requires `app:create`, accepts `{ slug, display_name, platforms }`, and SHALL — in a single transaction — create the `app` row plus three `channel` rows (`dev`, `beta`, `stable`) with `stable.is_default = true`. `slug` SHALL be unique within the org and immutable after creation. `PATCH /api/v1/apps/:slug` (requires `app:update`) SHALL only mutate `display_name`, `platforms`, or the default channel — never `slug`. `DELETE /api/v1/apps/:slug` (requires `app:delete`) SHALL succeed only when the app has zero releases, otherwise return `409` `type=app_has_releases`. App create and delete SHALL write an `audit_log` row.

#### Scenario: Creating an app seeds three channels

- **GIVEN** an authenticated principal holding `app:create`
- **WHEN** it POSTs `/api/v1/apps { slug: "swarmdrop", display_name: "SwarmDrop", platforms: ["tauri-desktop"] }`
- **THEN** the response is `201` with the created app
- **AND** three `channel` rows exist for the app named `dev`, `beta`, `stable`
- **AND** the `stable` channel has `is_default = true`
- **AND** an `audit_log` row records the app creation

#### Scenario: Duplicate slug within org is rejected

- **GIVEN** an app with slug `swarmdrop` already exists in the org
- **WHEN** a principal POSTs another app with slug `swarmdrop`
- **THEN** the response is `409` `type=conflict`
- **AND** no second app row is created

#### Scenario: Deleting an app with releases is blocked

- **GIVEN** an app `swarmdrop` that has at least one release
- **WHEN** a principal holding `app:delete` DELETEs `/api/v1/apps/swarmdrop`
- **THEN** the response is `409` `type=app_has_releases`
- **AND** the app and its releases remain

#### Scenario: slug is immutable

- **WHEN** a principal PATCHes `/api/v1/apps/swarmdrop` with a body attempting to change `slug`
- **THEN** the `slug` is unchanged (the field is ignored or rejected) while `display_name` / `platforms` updates apply

### Requirement: Server SHALL manage Channels under an App

The server SHALL expose `GET /api/v1/apps/:slug/channels` (`app:read`), `POST /api/v1/apps/:slug/channels` (`app:update`), and `PATCH /api/v1/apps/:slug/channels/:name` (`app:update`). Channel `name` SHALL be unique within the app. Setting a channel as default SHALL unset the previous default in the same transaction. Channel operations are gated by `app:update` (there is no `channel:*` permission). No channel DELETE endpoint is provided.

#### Scenario: Listing channels returns the seeded set

- **GIVEN** a freshly created app
- **WHEN** a principal holding `app:read` GETs `/api/v1/apps/:slug/channels`
- **THEN** the response lists `dev`, `beta`, `stable`

#### Scenario: Promoting a channel to default unsets the old default

- **GIVEN** an app whose default channel is `stable`
- **WHEN** a principal holding `app:update` PATCHes channel `beta` with `is_default = true`
- **THEN** `beta.is_default` becomes `true`
- **AND** `stable.is_default` becomes `false`

### Requirement: Server SHALL manage Release lifecycle draft → published → yanked

The server SHALL expose release endpoints under `/api/v1/apps/:slug/releases`. `POST` (requires `release:create`) creates a release with `status='draft'`; `version` SHALL be unique within the app; the body MAY include `android_version_code`. `POST .../:version/publish` (requires `release:publish`) flips `draft → published` and sets `published_at`; it SHALL NOT touch any channel pointer. `POST .../:version/yank` (requires `release:yank`) flips `published → yanked` and SHALL NOT auto-revert any channel pointing at it. Releases SHALL never be deleted. publish / yank SHALL write an `audit_log` row.

#### Scenario: Developer can create a draft but cannot publish

- **GIVEN** a principal holding `release:create` but not `release:publish` (the `developer` role)
- **WHEN** it POSTs a new release to `/api/v1/apps/swarmdrop/releases { version: "0.4.5" }`
- **THEN** the response is `201` with `status='draft'`
- **WHEN** it then POSTs `/api/v1/apps/swarmdrop/releases/0.4.5/publish`
- **THEN** the response is `403` `type=forbidden` carrying `required_permission: "release:publish"`
- **AND** the release remains `status='draft'`

#### Scenario: Publishing flips status and records audit

- **GIVEN** a draft release `0.4.5` and a principal holding `release:publish`
- **WHEN** it POSTs `/api/v1/apps/swarmdrop/releases/0.4.5/publish`
- **THEN** the response is `200` with `status='published'`
- **AND** `published_at` is set
- **AND** no `channel_release` pointer changed
- **AND** an `audit_log` row records the publish

#### Scenario: Duplicate version within app is rejected

- **GIVEN** a release `0.4.5` already exists for app `swarmdrop`
- **WHEN** a principal POSTs another release with `version: "0.4.5"`
- **THEN** the response is `409` `type=conflict`

#### Scenario: Yank does not move channel pointers

- **GIVEN** release `0.4.5` is published and the `stable` channel points at it
- **WHEN** a principal holding `release:yank` POSTs `.../releases/0.4.5/yank`
- **THEN** the response is `200` with `status='yanked'`
- **AND** the `stable` channel's `channel_release` pointer still references `0.4.5` (reverting is rollback's job, not yank's)

### Requirement: Server SHALL promote and rollback channels via pointer + append-only history

A channel SHALL track its current release through a `channel_release` row (`channel_id` primary key → `release_id`), at most one per channel. `POST /api/v1/apps/:slug/channels/:name/promote` (requires `release:promote`, body `{ version }`) SHALL — in one transaction — upsert the `channel_release` pointer to the named published release, append a `channel_release_history` row with `action='promote'`, and write an `audit_log` row. `POST .../rollback` (requires `release:rollback`, body `{ version? }`) SHALL repoint the channel: to the given `version` when supplied, otherwise to the immediately previous distinct release in that channel's history; when no prior history exists it SHALL return `422` `type=nothing_to_rollback`. Releases SHALL never be deleted by promote or rollback. The same release MAY be pointed at by multiple channels simultaneously.

#### Scenario: Promote points the channel and appends history

- **GIVEN** published release `0.4.5` of app `swarmdrop` and a principal holding `release:promote`
- **WHEN** it POSTs `/api/v1/apps/swarmdrop/channels/stable/promote { version: "0.4.5" }`
- **THEN** the response is `200`
- **AND** the `stable` channel's `channel_release` points at `0.4.5`
- **AND** a `channel_release_history` row exists with `action='promote'`, `release_id` of `0.4.5`, and the actor
- **AND** an `audit_log` row records the promote

#### Scenario: Same release promoted across channels without re-upload

- **GIVEN** release `0.4.5` already promoted to `beta`
- **WHEN** a principal promotes `0.4.5` to `stable`
- **THEN** both `beta` and `stable` `channel_release` rows reference `0.4.5`
- **AND** no artifact rows are duplicated or modified

#### Scenario: Rollback without version reverts to previous history entry

- **GIVEN** the `stable` channel was promoted to `0.4.4` then `0.4.5` (two history rows)
- **WHEN** a principal holding `release:rollback` POSTs `.../channels/stable/rollback` with no `version`
- **THEN** the response is `200`
- **AND** the `stable` `channel_release` points back at `0.4.4`
- **AND** a `channel_release_history` row with `action='rollback'` is appended
- **AND** release `0.4.5` still exists (not deleted)

#### Scenario: Rollback with no prior history is rejected

- **GIVEN** a channel that has never been promoted (no `channel_release_history`)
- **WHEN** a principal POSTs `.../rollback` with no `version`
- **THEN** the response is `422` `type=nothing_to_rollback`

### Requirement: Server SHALL expose read-only Artifact and current-release queries

The server SHALL expose `GET /api/v1/apps/:slug/releases/:version/artifacts` (requires `artifact:read`) returning the artifacts of a release, and `GET /api/v1/apps/:slug/channels/:name/release` (requires `release:read`) returning the release a channel currently serves (or empty when the channel has never been promoted). The server SHALL NOT expose artifact create/delete endpoints in this capability (artifact creation is the upload `complete` callback's job or the external-registration path, deferred to storage). Artifact listings SHALL include each artifact's available delivery locations (S3 presence and any `mirror_url`).

#### Scenario: Listing artifacts of a release

- **GIVEN** a release with artifacts present
- **WHEN** a principal holding `artifact:read` GETs `.../releases/:version/artifacts`
- **THEN** the response lists the artifacts with platform / target / arch / abi / filename / size_bytes / sha256
- **AND** each artifact indicates its delivery locations (S3 and/or `mirror_url`)

#### Scenario: Querying a never-promoted channel's current release

- **GIVEN** a channel with no `channel_release` row
- **WHEN** a principal GETs `/api/v1/apps/:slug/channels/:name/release`
- **THEN** the response indicates no current release (empty / 204-style payload), not a 404 on the channel

### Requirement: CLI SHALL provide read-only listing commands

The `swarmhive` CLI SHALL provide `apps list`, `releases list --app <slug>`, and `artifacts list --app <slug> --version <v>`, rendering human-readable tables by default and machine JSON under `--output json`. These commands SHALL call the corresponding GET endpoints using the configured credentials. Write commands (publish / promote / rollback) are NOT part of this capability.

#### Scenario: apps list renders a table

- **GIVEN** a logged-in CLI with credentials for a server having at least one app
- **WHEN** the user runs `swarmhive apps list`
- **THEN** the CLI prints a table of apps (slug / display_name / platforms / default channel)

#### Scenario: JSON output is machine-readable

- **WHEN** the user runs `swarmhive releases list --app swarmdrop --output json`
- **THEN** the CLI prints a JSON array of releases to stdout with no decorative table framing

### Requirement: Artifact 写入对并发多 target 安全

server 写入 artifact 行 SHALL 是数据库层面的原子 upsert(`INSERT ... ON CONFLICT (release_id, platform, target, arch, abi) DO UPDATE`),不得使用应用层 SELECT-then-INSERT。多个 target 并发写入同一 `(app, version)` release 时,所有 target 的 artifact MUST 全部持久化,任何一个都不得因写-写竞争而静默丢失。

#### Scenario: 多 target 并发上传同一 version 全部留存
- **WHEN** 4 个不同 target(如 aarch64-apple-darwin / x86_64-apple-darwin / x86_64-unknown-linux-gnu / x86_64-pc-windows-msvc)并发对同一 `(app, version)` 完成上传
- **THEN** 该 release 下 MUST 存在 4 个 artifact 行,每个 target 各一,无丢失、无静默覆盖

#### Scenario: 同 target 重传是幂等 upsert
- **WHEN** 同一 `(release, platform, target, arch, abi)` 被重复上传(重跑/补传)
- **THEN** server MUST 更新该行的内容列(filename/size/sha256/storage_backend_id/object_key),而不是新增重复行,且不报冲突错误;签名(signature_metadata)**仅在本次请求带签名时**才覆盖,不带签名的重传保留既有签名(幂等 re-complete 不抹除已有签名,见 design D1)

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

### Requirement: Artifact SHALL support one or more delivery locations

An artifact SHALL model its bytes as one or more delivery locations rather than a single mandatory S3 object. `storage_backend_id` and `object_key` SHALL be nullable (present together for an S3-backed object, absent otherwise), and an optional `mirror_url` SHALL hold an external delivery location (currently the GitHub Release asset URL). Every artifact MUST have at least one delivery location — an S3 object, a `mirror_url`, or both. The identity/uniqueness key of an artifact `(release_id, platform, target, arch, abi, kind)` SHALL be unchanged; delivery-location columns are descriptive and MUST NOT be part of that key. `api::Artifact` SHALL reflect the nullable S3 fields and the optional `mirror_url`.

#### Scenario: S3-backed and GitHub-only artifacts coexist under one release

- **WHEN** one artifact is uploaded to S3 and another is registered with only a `mirror_url`
- **THEN** both persist under the same release, each with at least one delivery location
- **AND** neither violates the `(release_id, platform, target, arch, abi, kind)` uniqueness key

#### Scenario: An artifact with no delivery location is invalid

- **WHEN** an attempt is made to persist an artifact with neither an S3 object nor a `mirror_url`
- **THEN** it is rejected (no delivery location)

