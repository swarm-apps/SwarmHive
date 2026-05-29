# design — add-cli-management-commands

CLI-only(消费 api-types + HTTP,**不依赖 entity/sea-orm**)。design 聚焦命令树、命令↔端点映射、`--output json` / 错误契约、client helper 补充、命名决策。

## 命令 ↔ 端点映射(全部已存在,零后端改动)

```text
  swarmhive-cli (clap)                        swarmhive-server (HTTP)
  ┌──────────────────────────────┐
  │ apps     list                │──GET    /api/v1/apps
  │          get      --app      │──GET    /api/v1/apps/{slug}
  │          create   …          │──POST   /api/v1/apps
  │          update   --app …    │──PATCH  /api/v1/apps/{slug}
  │          delete   --app --yes│──DELETE /api/v1/apps/{slug}            (有 release → 409 app-has-releases)
  │ channels list     --app      │──GET    /api/v1/apps/{slug}/channels
  │          create   --app --name──POST   /api/v1/apps/{slug}/channels
  │          set-default --app --name──PATCH /api/v1/apps/{slug}/channels/{name}  { is_default:true }
  │          promote  --app --name --version──POST /…/channels/{name}/promote
  │          rollback --app --name [--to-version]──POST /…/channels/{name}/rollback
  │ releases list     --app      │──GET    /api/v1/apps/{slug}/releases
  │          get      --app --version──GET  /…/releases/{version}
  │          create   --app --version [--android-version-code] [--notes-file]──POST /…/releases
  │          update   --app --version …──PATCH /…/releases/{version}
  │          publish  --app --version──POST  /…/releases/{version}/publish
  │          yank     --app --version --yes──POST /…/releases/{version}/yank
  │ artifacts list    --app --version──GET   /…/releases/{version}/artifacts
  └──────────────────────────────┘
   auth: SWARMHIVE_TOKEN env > credentials.toml(已有 auth::resolve);server: env > swarmhive.toml > creds
```

## Goals / Non-Goals

**Goals:** apps/channels/releases 管理与 Web Admin 对齐;`--output json` 成功 + problem+json 错误 + 非零 exit + `--yes` 的稳定契约(供配套 skill 解析);沿用既有 `<名词> <动词>` + `OutputFormat` + `emit` 范式。

**Non-Goals:** storage/mail(proposal 2)、users/tokens、MCP、artifact 删除、改 `publish {tauri|android}`。

## Decisions

### D1. `channels` 名词组,收编 top-level `promote`/`rollback` 桩

现有 `promote`/`rollback` 是 top-level `todo!()` 桩(从未发布)。改放进 `channels {promote,rollback}`,与 apps/releases 的 noun-verb 一致,且 `channels` 还要装 `list`/`create`/`set-default`。**Why**:AI / skill 靠可预测的 noun-verb 语法拼命令;把 channel 操作集中一组比散落 top-level 更可推断。`publish --channel` 的便捷 promote-after 保留不变。**备选**(保留 top-level promote/rollback)弃:与新 channel 组割裂。

### D2. `releases publish`(发布 draft) vs `publish {tauri|android}`(上传式发布)

两个不同操作并存:`publish tauri`/`publish android` 是「扫 bundle → 上传 → complete(默认发布)」的端到端;`releases publish --app --version` 是「把一个已存在的 draft 置 published」(`POST /…/publish`,不上传)。**Why**:AI 工作流常是「先 `releases create` 建 draft → 检视 → 再 `releases publish`」的细粒度;上传式 `publish` 是一站式便捷。docs 明确区分,命名不强行统一(强行合并会牺牲两种真实用法)。

### D3. `--output json` 贯穿写操作 + problem+json 错误契约(本 proposal 的 AI 核心)

```text
成功:  --output json → 结果对象/数组打到 stdout(create→新建的 App、promote→更新后的 Release …)
       --output table(默认)→ 人类表格(沿用 emit)
错误:  API 4xx/5xx → 解析 RFC 9457 problem+json,--output json 时原样打到 stderr;
       table 时打人话(problem.detail);两种都 **非零 exit code**
```

- `client.rs` 现状:非 2xx 由各 helper `anyhow::bail!` 成人话串,丢失结构。改:helpers 在非 2xx 时解析 problem+json 成结构化 `ApiError { status, type, title, detail, extra }`(login.rs 已有手解 `detail` 的雏形,提炼复用),用 `thiserror` 表达。
- `main.rs` 顶层:把 `Cli::parse()` 后的 `run()` 结果在 main 里按 `cli.output` 渲染——`Err(ApiError)` 且 `output=json` → `eprintln!` problem+json + `std::process::exit(非零)`;否则人话。**Why**:skill 只认「stdout=成功 JSON / stderr=problem JSON / exit code」一套契约就能稳稳包住整个 CLI。

### D4. 破坏性操作 `--yes`(非交互显式意图)

`apps delete` / `releases yank` 必须带 `--yes`,否则报错退出(不交互弹确认——CLI 要全非交互供 AI 用)。**Why**:配合最小权限 API Token(token 无 `app:delete` 则后端 403),`--yes` 是客户端侧的「显式意图」第二道闸。

### D5. client.rs 补 `patch_json` / `delete_no_content`

当前只有 `get_json` / `post_json` / `post_ensure` / `upload_put`。补:`patch_json<B,T>`(update app/release/channel)、`delete_no_content`(DELETE → 204)。沿用同一 `build_client` + bearer 注入 + 错误解析路径。

### D6. 输入约定

- `--platforms tauri-desktop,react-native-android`(逗号分隔,解析成 `Vec<Platform>`)。
- `--notes-file <path>`(release notes 走文件,避免多行塞 flag;与未来模板同范式)。
- slug 不可变:`apps update` 不接受改 slug(后端也不允许)。
- `rollback` 的 `--to-version` 可选:省略 = 回退到上一个 distinct release(后端语义),无历史 → `422 nothing-to-rollback`。

## Risks / Trade-offs

- [`releases publish` 与 `publish` 命名歧义] → docs/12 显式对照表 + 各自 `--help` 写清;不强行改名(改名会动既有 `publish` 用户习惯)。
- [错误结构化重构波及所有 helper] → 集中在 `client.rs` 一处;`ApiError` 提炼自 login.rs 已有逻辑,回归用单测锁解析。
- [AI 误用破坏性命令] → `--yes` + 最小权限 token 双闸;docs 建议给 AI 的 token 不含 `app:delete`/`release:yank`。
- [CLI 边界回归] → `cargo tree -p swarmhive-cli | grep sea-orm` 必须仍空(只加 api-types + reqwest 用法)。

## Migration Plan

纯增量。移除 top-level `Promote`/`Rollback` 桩(从未发布,无兼容包袱)。新命令不影响既有 `publish`/`verify`/`storage`/`login`。错误输出从人话变「table 人话 / json problem」——table 默认行为对人类基本不变。

## Open Questions

- `channels get`(看某 channel 当前指向)是否要单列?后端有 `GET /…/channels/{name}/release`。倾向**加进 `channels list` 的输出**(每行带 current version),不单列命令,除非 apply 时发现单查更顺手。
