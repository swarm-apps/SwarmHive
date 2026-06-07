## Context

`swarmhive-cli` 的发布链路(`commands/publish.rs`、`verify.rs`)与项目骨架(`commands/project.rs` + `config.rs`)已落地,但有 4 处与 docs/12-cli.md 契约的偏差需要补齐,且 `main.rs` 的 `Command::Init` 仍是 `todo!`。

当前事实(读码确认):

- `publish.rs` 的 `run()` 用 `CreateReleaseRequest { release_notes: None, .. }`(写死),无 notes flag。
- `publish` 的 `CommonArgs` 无 `dry_run` / `output`;`verify.rs` 有 `--dry-run` 但无 `--output`。
- `main.rs` 有全局 `--output {table|json}`,但**未透传**给 `publish`/`verify` 的派发。
- CLI **无重量级交互库**(只有 `rpassword` + 手动 stdin);`std::io::IsTerminal`(std,MSRV 1.90 可用)可做 TTY 判定,无需新依赖。
- 进度条在 `client.rs::upload_put` 内创建,无 TTY 判定。
- release update 端点已存在(`releases update --notes-file` 已用 `UpdateReleaseRequest.release_notes`),可复用注入 notes,**零 server 改动**。

约束:仅改 `crates/swarmhive-cli`;不碰 server/entity/api-types/schema;不破坏既有命令行为(纯加性 flag)。

## Goals / Non-Goals

**Goals:**

- `swarmhive init` 可用:交互式(TTY)生成 `swarmhive.toml`,非 TTY 不阻塞。
- `publish` 能注入 changelog(`--notes-file`/`--notes`),对新建与既有 release 都生效。
- `publish` 支持 `--dry-run`(本地预检、零网络)。
- `publish`/`verify` 成功时输出合法 JSON(`--output json`),与管理命令契约一致。
- 进度条仅在交互 TTY 且非 JSON 模式渲染。

**Non-Goals:**

- 不加 server 端点、实体、schema;不改 presign/complete 协议。
- 仅为 `init` 引入 `dialoguer`(与 `indicatif` 共享 `console`);不引 `inquire`/`cliclack`。
- `init` 纯本地、不联网(不拉 `/apps` 列表);slug 走探测 + free-text 默认。
- 不做多 ABI 自动识别(`fat APK` abi=None 已默认支持)。
- 不改 GitHub Action 逻辑(它已用 `dry-run→verify` verb 切换)。

## Decisions

### D1 — notes 注入走 "ensure 后按需 PATCH",复用 release update 端点

`publish` 提供 `--notes-file <path>`(读文件)或 `--notes <text>`(取其一,file 优先);二者皆无则维持现状(不动 notes)。注入路径:

```text
swarmhive publish tauri --notes-file CHANGELOG.md --channel stable
  │
  ├─ plan_artifacts(本地: 定位产物 + sha256 + 找 .sig)
  │
  ├─[--dry-run]→ 打印计划 → return(不发任何请求)        ← D2
  │
  ├─ POST /apps/:slug/releases            ensure draft(created? 409=已存在)
  │     └─ CreateReleaseRequest.release_notes = notes  (新建一步到位)
  │
  ├─[notes 且 created==false]
  │     └─ PATCH /apps/:slug/releases/:ver  UpdateReleaseRequest.release_notes = notes  (既有 release 补更新)
  │
  ├─ POST …/uploads/presign  →  PUT ×N(进度条, D4)  →  POST …/complete(publish=true)
  │
  └─[--channel]→ POST …/channels/:c/promote
```

- 新建 release:notes 随 `CreateReleaseRequest` 一次写入(省一次请求)。
- 既有 release(409):再发一次 `PATCH` 更新 notes(幂等)。
- **备选(弃)**:总是 PATCH——多一次请求且对新建冗余。**备选(弃)**:只在 create 写——既有 release 永远更新不到 notes(回归)。

### D2 — `--dry-run` = 纯本地预检,零网络、免鉴权

`publish --dry-run` 只跑 `plan_artifacts`(定位文件、算 sha256、查同名 `.sig`),打印"将要发布的 release / 产物 / 目标 channel / 是否带签名"计划后返回,**不调 `require_creds`、不发任何 HTTP**。

- 理由:dry-run 的核心价值是"我会上传什么",本地即可回答;server 侧的"重复版本"预检由 `verify`(已实现,会查 server)承担,Action 也正是用 `dry-run→verify` 覆盖该场景。两者职责互补、不重叠。
- `--dry-run` 与 `--output json` 组合时,输出 `{ "dry_run": true, "app", "version", "artifacts":[…] }`。

### D3 — `--output json` 透传 + 成功负载形状

`main.rs` 已有的全局 `OutputFormat` 透传进 `publish`/`verify` 派发。成功负载:

- `verify`: `{ "app"?, "version"?, "version_code"?, "artifacts": [{ "path", "size", "sha256" }], "ok": true }`
- `publish`: `{ "app", "version", "status", "published": bool, "channel"?, "artifacts": [{ "filename", "size", "sha256", "signed": bool }], "endpoints": { <platform>: <url> } }`

失败路径不变(已走 `client.rs::render_error` 的 RFC 9457 problem+json → stderr + 非零退出)。`table`(缺省)维持现有人类可读输出。用 `serde_json` 序列化(已是依赖)。

### D4 — 进度条 TTY/JSON 守卫

`upload_put` 接收的 `ProgressBar` 在**无 TTY**(`!std::io::stderr().is_terminal()`)或 `--output json` 时用 `ProgressDrawTarget::hidden()` 创建(indicatif 原生支持),其余路径不变。判定放在 `publish.rs` 构造 `progress_bar()` 处,`upload_put` 签名不动。

### D5 — `init` 双模式(交互 + 命令式),`dialoguer` + 手写 toml 模板

库选型:**`dialoguer`**(console-rs,与已有 `indicatif` 共享 `console` crate → 新增传递依赖极低)。**纯本地、不联网**(用户拍板)。`std::io::IsTerminal` 做 TTY 判定(std,已在 `client.rs` 用)。

**所有字段都有对应 flag**(命令式入口):`--server`、`--app <slug>`、`--platform <tauri|android>`(可重复)、`--tauri-conf <path>`、`--android-apk <path>`、`--force`、`--yes`(非交互)、全局 `--output {table|json}`。**flag 永远覆盖 prompt/默认**。

**模式选择**:

- **交互模式**(TTY 且未传 `--yes`):对**未由 flag 给出**的字段用 `dialoguer` 逐项 prompt——`server`(`Input`,默认取全局 `credentials.toml` 的 server,空→不写)、`app.slug`(`Input`,默认 cwd 目录名 kebab 化)、平台(`MultiSelect`,按探测预勾:有 `src-tauri/`→tauri、有 `android/` 或顶层 `*.gradle*`→android)、`tauri.conf`/`android.apk`(`Input` 带默认)。
- **命令式 / 非交互模式**(`--yes` 或非 TTY):**绝不 prompt**;每个字段取 flag → 探测默认;平台未给则用探测结果;**唯一硬性必填是 `app.slug`**(无 `--app` 且无法从 cwd 推断合法值时报错)。错误走 RFC 风格(`--output json` 时 problem+json → stderr)+ 非零退出。**这让 AI/skill/CI 能无人值守初始化**,与管理命令的非交互契约(env token + 全 flag,见 `cli-management`)一致。
- `artifacts` 两模式都**不 prompt**,统一生成**带注释的示例块**让用户后填。
- `--output json` 成功输出单对象 `{ path, app, server?, platforms[], created: true }`;table 模式打印写入路径 + "记得填 artifacts" 提示。

**生成方式 = 手写字符串模板**(非 serde 序列化):① toml 序列化器丢注释,而 artifacts 示例 + 字段说明需要注释;② 避免给只读 `Deserialize` 的 `ProjectConfig` 加 `Serialize`。模板产物**必须能被 `ProjectConfig::load` 解析**(写完做一次自解析校验)。写到项目根 `swarmhive.toml`;已存在则拒绝,除非 `--force`。

### D6 — 交互框架统一到 `dialoguer`,移除 `rpassword`

CLI 仅存的手写交互是 `client.rs::resolve_secret` 的 TTY 分支(`rpassword::prompt_password`),被两处 `create` 调用:`storage create`(`storage.rs:209`,`Some("Access key secret: ")`)与 `mail providers create`(`mail.rs:243`,`Some("SMTP password ...")`);两处 `update` 传 `None` 不提示。**注**:登录早已是 RFC 8628 device flow(`login.rs` 无密码、不依赖 `rpassword`),故这两处录密钥是 `rpassword` **仅存的活跃用途**。

把该分支换成 `dialoguer::Password::new().with_prompt(..).interact()`(同样不回显),并从 root + cli `Cargo.toml` 移除 `rpassword` → CLI 只剩 `dialoguer` 一个交互库。

- **契约不变**:`--secret-stdin` > env > 明文 flag > TTY 提示 四级优先级、`create` 提示 / `update` 省略=保留语义、AI 走 stdin/env 全部照旧;`storage-cli-admin`/`mail-cli-admin` 的 spec 不动(纯实现层重构)。
- **保留 `is_terminal()` 外层预判**:`dialoguer` 的 `Password::interact()` 在非 TTY 会自报错,但本函数语义是"无 TTY → 返回 `None`(不提示)";继续用既有 `std::io::stdin().is_terminal()` 守卫该分支,只在 TTY 才走 dialoguer。`dialoguer::Error` map 进 `anyhow`。
- 备选(弃):保留 `rpassword` —— CLI 同时背 `rpassword` + `dialoguer` 两个交互库,与"统一框架"目标相悖。

## Risks / Trade-offs

- [release update 端点字段不符] notes 注入依赖 `UpdateReleaseRequest.release_notes` 存在且 PATCH 路由可用 → 实施首步先核对 `api-types` 与 server 路由;若缺(应不缺,`releases update` 已用)则记风险、最小补,不扩面(见 Non-Goals)。
- [dry-run 不查重复版本] 纯本地 dry-run 不会发现"版本已发布" → 由 `verify` 兜底(文档里把 verify 标为 server 预检入口);可接受。
- [新增 `dialoguer` 依赖] → 与 `indicatif` 共享 `console`,边际依赖/编译开销低;只用在 `init` 一处,不外溢到其它命令。需在 workspace 根 `Cargo.toml` 的 `[workspace.dependencies]` pin 版本(项目约定)。
- [init 默认 app slug 取 cwd 目录名可能不合法] → 交互可改;纯本地不校验,生成后由后续 `publish` 的 server 校验兜底。
- [artifacts 只给注释模板、用户忘改] → 注释里写明"发布前需填实际产物路径";`publish` 在 artifacts 为空时已会报错指引。

## Migration Plan

纯加性,无迁移。新 flag 默认关闭/缺省即旧行为;`init` 此前是 `todo!`(panic),实现后是净增益。回滚 = revert commit。
