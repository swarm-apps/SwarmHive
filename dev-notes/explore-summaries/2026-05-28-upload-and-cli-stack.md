# Explore Summary — 上传链路 + CLI 技术栈（2026-05-28）

> explore 模式产出的临时决策档，给 `/opsx:propose add-storage-and-presign-upload` 和 CLI 推进引用。归档前不要 commit 到 `dev-notes/knowledge/`——决策落到具体 proposal 后本文件可删。与 [2026-05-27-account-onboarding.md](2026-05-27-account-onboarding.md) 同款临时档。

## 背景

`docs/12-cli.md` 已锁死上传**形态**（presign 直传 + complete 回调，CLI 不走 server 中转字节）、命令面（init / verify / storage init / publish tauri|android / promote / rollback / list）、UX（进度条 / `--dry-run` / `--output json` / CI 友好）。`config/project.md` 锁死 server 侧 `aws-sdk-s3 + presign`。

本次 explore 把 docs 没定的**技术栈实现细节**逐个拍板，覆盖上传完整性、上传粒度、CLI 网络/分发/输出/内省/重试。CLI 当前已有依赖：`clap + tokio + reqwest + indicatif + directories + rpassword + serde + toml + anyhow/thiserror`，仅 `login` / `logout` 落地，其余命令为 `todo!()` stub。

## 上传时序（含完整性闭环）

```text
CLI ──POST /api/v1/uploads/presign──▶ Server
       { app, version, channel, files[{name, sha256, size}] }
                                      aws-sdk-s3 生成 presigned PutObject
                                      把 expected sha256 绑进 x-amz-checksum-sha256
     ◀── { upload_id, parts[{ object_key, url, headers, expected_sha256 }] }

CLI ──PUT <signed url>  (单 PUT, streaming body + 进度条 + backon 重试)──▶ S3/RustFS/OSS
                                      S3 自算 sha256, 与签名内 checksum 不符 → 4xx 拒
     ◀── 200 + ETag

CLI ──POST /api/v1/uploads/{upload_id}/complete──▶ Server
       { parts[{ object_key, sha256, etag }] }
                                      HEAD object 读 checksum + size 确认
                                      写 release / artifact（ON CONFLICT 幂等）
     ◀── { release_id, endpoints: {...} }

客户端（updater/SDK）下载时再按 artifact.sha256 自校字节 ← 端到端完整性闭环
```

## 关键决策

| # | 决策点 | 选定 | 理由 |
|---|---|---|---|
| 1 | 完整性校验 | **S3 原生 SHA256 checksum**（`x-amz-checksum-sha256`） | presign 把 expected sha256 绑进签名，S3 收字节自算 sha256，不符直接拒 PUT。server 零字节、不信任 CLI 自报。ETag 对单 PUT 只是 MD5、不可用。 |
| 2 | 上传粒度 | **MVP 单 PUT**，失败整体重传 | 单 PUT 原生配合**整体** `x-amz-checksum-sha256`，正好是客户端下载要自校的值。S3 multipart 的对象 checksum 是 composite（`sha256-of-parts` + `-N` 后缀）≠ 整体 sha256，会逼退弱校验。产物 5–200MB 单 PUT 完全够（上限 5GB）。multipart / 断点续传 post-MVP。 |
| 3 | CLI TLS + 根证书 | **rustls + 系统根证书**（`rustls-tls-native-roots`） | 纯 Rust 无 OpenSSL，跨平台 / musl 静态编译省心；读系统信任库 → 尊重 self-host 用户的企业 / 已导入自签 CA。自签未进系统库 → `--ca-cert <pem>` / `SWARMHIVE_CA_CERT` 逃生口（`--insecure` 仅 dev + 显式警告）。 |
| 4 | 构建 + 分发 | **cargo-dist (dist) 一站式** | 一份 CI 配置产出：各平台 prebuilt 二进制（GH Releases + checksums）+ `curl\|iex` 安装脚本 + npm 包 `@swarmhive/cli`（`npx` 可跑，贴合 Tauri/RN 受众）+ 可选 homebrew。GitHub Action 只 download+exec 薄包装。npm 包由 dist 免费产出，不用手维护平台矩阵。 |
| 5 | 输出 | 人类侧 **tabled (derive)**；机器侧 `--output json`（serde_json）；颜色 `console`（已随 indicatif 间接引入） | DTO 加 `#[derive(Tabled)]` 零样板成表，与 list 命令 struct 贴合。 |
| 6 | verify/publish 内省深度 | **信任 flag + 轻 verify**（MVP） | verify = 文件存在 + version 重复检查（查 server）+ latest.json 可解析 + 算 sha256；versionName/Code 信任 `--flag`（docs publish android 本就显式传 `--version-code`）。不解析 APK 二进制 AXML、不客户端验 minisign → 避免引入不成熟的二进制解析依赖。深度内省（AXML 交叉校验 / minisign 验签）作为 verify 命令增强后续补。 |
| 7 | 上传重试 | **backon** | `.retry(ExponentialBuilder::default().with_jitter())` 包住 presign / PUT / complete；只重试 5xx / timeout / conn-reset，4xx（签名过期 / checksum 不符）直接报。complete 设计为幂等、PUT 同 key 幂等 → 重试安全。比手写循环多免费 jitter/退避。 |

## 无悬念已锁（不单列决策）

- **clap**：derive API + 嵌套子命令（`publish tauri|android`）。
- **async**：`#[tokio::main(flavor = "current_thread")]`——CLI 单流 IO，不需线程池。
- **config**：`swarmhive.toml` / `credentials.toml` 走 `toml::from_str`（不引 figment；CLI 已有 toml+serde）。
- **CLI 零 `aws-sdk-s3` 依赖**：presigned URL 由 server 生成，CLI 只对该 URL 做普通 `reqwest` PUT。回归测试 `cargo tree -p swarmhive-cli | grep aws-sdk` 应无输出（与现有 `grep sea-orm` 同款边界守护）。

## 新增依赖（待 propose 时进 `[workspace.dependencies]`）

- **server**：无新增（`aws-sdk-s3` 本就规划在 `add-storage-and-presign-upload`）。presign 用 aws-sdk-s3 的 `PutObject` + `presigned()` + `ChecksumSha256`。
- **CLI**：`sha2`（流式算 sha256）、`tabled`（人类表格）、`backon`（重试）；`reqwest` features 切 `rustls-tls-native-roots`。
- **构建侧**：`dist`（cargo-dist，开发工具，不入 runtime dep）。

## 线头决策（续探）

### 对象路径规范（配合指针模型，去 channel）

```text
apps/{app_slug}/versions/{version}/{platform}/{target}/{filename}
```

示例：

```text
apps/swarmdrop/versions/0.4.5/tauri/windows-x86_64/SwarmDrop_0.4.5_x64-setup.exe
apps/swarmnote-rn/versions/0.2.0/android/arm64-v8a/swarmnote-0.2.0-arm64.apk
```

- **不含 channel**：release 与 channel 解耦，对象上传一次，promote dev→beta→stable 只移 `channel_release` 指针、对象零动（与发布列车模型一致）。
- 路径是 **server 内部细节**、非公开契约：CLI 的 `object_key` 从 presign 响应拿；下载走 `GET /download/:app/:version/:artifact_id` 由 server 用 artifact_id 解析回 key，因此路径方案可随 server 内部演化。
- ⚠️ `docs/07` 文件路径规范段 + `memory/project-storage-model.md` 对象路径段是旧的带 `channels/{channel}` 路径，已过时。memory 本次同步改掉；`docs/07` 随 `add-storage-and-presign-upload` apply 时改（权威文档随代码走 diff gate）。

### swarmhive.toml schema

```toml
# 提交进仓库根；token 不放这里（走 credentials.toml / SWARMHIVE_TOKEN）
server = "https://updates.example.com"

[app]                       # 单 app；monorepo 边缘用 --app 覆盖，或各子目录各放一份
slug = "swarmdrop"
platform = "tauri"          # tauri | android

[app.tauri]
artifacts = "./src-tauri/target/release/bundle"
notes = "CHANGELOG.md"
# version 自动读 ./src-tauri/tauri.conf.json .version，--version 可覆盖

[app.android]
apk = "./android/app/build/outputs/apk/release/app-release.apk"
# version / version-code 必须显式 --flag（不解析 build.gradle / APK，与决策 6 一致）
```

- **单 app + `--app` 覆盖**：一文件一 app（swarm-apps 各产品独立仓）；CLI 默认读 `[app]`，`--app` 临时覆盖。
- **server 进 config，token 不进**：token 走 `credentials.toml`（login 写）或 `SWARMHIVE_TOKEN` env。
- **version 平台分治**：Tauri 自动读 `tauri.conf.json` `.version`（干净 JSON）+ `--version` 覆盖；Android 显式 `--version` / `--version-code`（build.gradle 是代码、APK AXML 不解析 → 与决策 6 一致）。

### GitHub Action 形态

- **composite action**（`action.yml` + shell steps）：GH runner 上 `npx @swarmhive/cli@<ver>`（或 cargo-dist 安装脚本）取 CLI → inputs 映射成 CLI flags；`SWARMHIVE_TOKEN` 走 secret。薄、快、与 cargo-dist 的 npm 产物天然配合。
- 否决 **Docker action**（仅 Linux runner，与 Tauri 桌面常在 mac/win runner 构建冲突）；否决 **JS action**（比 composite+npx 多一层要维护/bundle 的 node 代码，收益不明显）。
- 待定 logistics：Action 放独立仓（marketplace tag 版本）还是本仓 `action.yml` + 发布 workflow —— propose 时定。

## 落到哪些 proposal

| 决策 | 消费 proposal |
|---|---|
| 1 / 2（完整性 + 单 PUT）+ presign/complete 端点 | `add-storage-and-presign-upload`（含 `StorageBackend` 实体 + `storage` trait + S3 实现） |
| 3 / 4 / 5 / 7（TLS / 分发 / 输出 / 重试） | CLI 工具链建设（分发可单列 `add-cli-distribution` 或并入 storage proposal 的 CLI 段） |
| 6（verify 深度） | CLI `verify` 命令（随 `publish` 一起，在 storage proposal） |
| list 命令（apps/releases/artifacts） | 已在 `add-app-release-artifact`（只读，不依赖上传） |

## 待 propose 时再展开的点（本次未深入）

- `swarmhive.toml` schema（app / channel / artifacts 路径 / notes 约定）。
- 对象路径规范（见 `docs/07` 文件路径规范段 + `memory/project-storage-model.md`）与 presign `object_key` 生成规则。
- `publish` 前「≥1 artifact」校验时机（`add-app-release-artifact` 的 design Open Question，落在 storage proposal）。
- GitHub Action 形态（下载 dist 二进制 → 注入 server/token → 调 publish）。
- 老 Aliyun OSS S3-compat 模式对 `x-amz-checksum-sha256` 的支持度核实（决策 1 的前提；不支持需回落策略）。

## maps to docs

- [docs/12-cli.md](../../docs/12-cli.md) 命令设计 + 上传形态
- [docs/07-storage-and-delivery.md](../../docs/07-storage-and-delivery.md) S3-only + 文件路径规范
- [docs/06-cicd.md](../../docs/06-cicd.md) GitHub Action 薄包装
- `openspec/changes/add-storage-and-presign-upload/proposal.md`、`openspec/changes/add-app-release-artifact/`
