# CLI 设计

## 定位

SwarmHive CLI 是开发者本地发布、CI/CD 自动发布和日常运维操作的统一入口。

它不是 CI/CD 的附属品，而是 SwarmHive 的一等入口。Web Admin 适合查看、配置和排障；CLI 更适合上传产物、校验发布和执行自动化操作。

## 使用场景

### 本地手动发布

用户本地构建完成后，直接上传产物：

```bash
swarmhive publish tauri
swarmhive publish android
```

适合：

- 个人项目。
- 小团队手动发布。
- 临时热修版本。
- CI/CD 尚未配置完成的早期阶段。

### CI/CD 自动发布

GitHub Actions、GitLab CI、Gitea Actions、Jenkins 调用同一套 CLI：

```bash
swarmhive publish tauri --channel stable --notes-file CHANGELOG.md
```

### 运维操作

- 查看版本。
- promote。
- rollback。
- yanked 某个版本。
- 验证产物完整性。

## 配置文件

项目根目录可放置 `swarmhive.toml`：

```toml
# server 可选;缺省回退到 `swarmhive login` 写的凭据里的 server。
server = "https://updates.example.com"

[app]
slug = "swarmdrop"

[app.tauri]
conf = "src-tauri/tauri.conf.json"   # release 版本从这里自动读取
artifacts = [
  "src-tauri/target/release/bundle/msi/SwarmDrop_0.4.5_x64_en-US.msi",
  "src-tauri/target/release/bundle/msi/SwarmDrop_0.4.5_x64_en-US.msi.zip",
  "latest.json",
]

[app.android]
apk = "app/build/outputs/apk/release/app-release.apk"
```

> channel 不在配置文件里(无 `default_channel` 字段),发布时用 `publish --channel <name>` 指定。

CLI 参数优先级：

1. 命令行参数。
2. 环境变量。
3. `swarmhive.toml`。
4. CLI 全局配置。

## 认证

支持（`add-pat-and-api-token` 落地）：

- `swarmhive login [server]` —— **RFC 8628 device flow**（`gh` 风格，CLI 不经手密码；初版用 `cli-token` ROPC，由 `add-cli-device-login` 替换）：调 `POST /api/v1/auth/device/code` 拿 `user_code` + `verification_uri`，打印并尝试打开浏览器到 `{base_url}/device`；用户在 Web 登录（密码 或 GitHub）后批准，CLI 轮询 `POST /api/v1/auth/device/token` 换 PAT；拿到后调 `/auth/me` 取 email，把 `{server, email?, token}` 写入 `~/.config/swarmhive/credentials.toml` 并 chmod `0600`（unix；Windows 走默认 ACL）。OAuth-only 用户也能用 CLI（认证在浏览器侧）。
- `swarmhive logout` —— 服务端 DELETE 当前 PAT（按 prefix 匹配自动定位 token id），再删本地文件。server 离线只 warn + 清本地。
- `SWARMHIVE_TOKEN` 环境变量 —— 优先级最高，覆盖 credentials 文件。CI/CD 注入 secret 即用。
- CI 推荐使用 API Token（scoped）而非 PAT；本地开发用 PAT。

示例：

```bash
# 本地登录（默认 http://localhost:3030；后续命令直接读 credentials.toml）
swarmhive login

# 远程 server + 显式 email
swarmhive login https://updates.example.com --email release@swarm-apps.dev

# CI：纯 env，不写文件
export SWARMHIVE_TOKEN=swhv_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
swarmhive publish tauri --app swarmdrop --version 0.4.5
```

Token 字符串格式：`swhv_pat_<43>`（个人）/ `swhv_api_<43>`（机器）。前缀公开仅是 metadata 便于日志 grep，无安全意义。

凭证文件位置（按 OS）：

- macOS：`~/Library/Application Support/dev.swarmhive.swarmhive/credentials.toml`
- Linux：`~/.config/swarmhive/credentials.toml`
- Windows：`%APPDATA%\swarmhive\swarmhive\config\credentials.toml`

参数优先级（与配置文件相同）：

1. 命令行参数 / 显式 env
2. `SWARMHIVE_TOKEN`
3. `~/.config/swarmhive/credentials.toml`
4. CLI 全局配置

## 命令设计

### init

```bash
swarmhive init                                                        # 交互式(TTY,dialoguer)
swarmhive init --app swarmdrop --platform tauri --yes --output json   # 命令式 / 非交互(AI/CI)
```

生成 `swarmhive.toml`,**双模式**:TTY 且无 `--yes` 时用 dialoguer 对**缺失字段**交互 prompt(平台按 `src-tauri/`/`android/` 探测预勾);`--yes` 或非 TTY 时**绝不 prompt**,纯靠 flag + 探测默认生成(供 AI / skill / CI 无人值守驱动),仅缺 `--app` 且无法从目录名推断时报错。flag 永远覆盖 prompt / 默认。已存在不覆盖(除非 `--force`)。纯本地、不联网。

### verify

```bash
swarmhive verify tauri
swarmhive verify android
```

校验：

- 版本号。
- 产物存在性。
- Tauri latest.json。
- Tauri signature。
- target / arch。
- APK versionName / versionCode。
- 重复发布。

### storage init rustfs

```bash
swarmhive storage init rustfs
```

用于 single-server 模式，输出或执行官方 bundled RustFS 部署指引。

能力：

- 生成 RustFS 所需环境变量。
- 输出 Docker Compose profile 命令。
- 检测 RustFS endpoint 健康状态。
- 测试 bucket、上传和下载。
- 将结果写入 SwarmHive storage 配置。

### 上传形态：presign 直传 + complete 回调

`publish tauri` 与 `publish android` 共用一套上传流程，CLI 不走 server 中转，直接把字节 PUT 到对象存储：

```text
CLI                                      Server                          S3 / RustFS / OSS
 │  POST /api/v1/apps/{slug}/releases/{ver}/uploads/presign  ───▶  │
 │    { files: [{ relative_path, size,   │  artifact:upload check
 │      expected_sha256, platform,       │  release 须已存在（draft）
 │      target?, arch?, abi? }] }        │  生成 per-file presigned PUT
 │  ◀───  { upload_id, parts: [          │  绑 x-amz-checksum-sha256
 │           { object_key, presigned_url, │
 │             headers } ] }              │
 │                                                                       │
 │  PUT  <presigned_url>  (stream bytes + 回放 headers)  ───────────────▶│
 │  ◀────────────────────────────────────  S3 自算 sha256，不符 4xx 拒  │
 │                                                                       │
 │  POST .../uploads/{upload_id}/complete  ───▶  server HeadObject 校验   │
 │    { parts: [{ object_key, sha256 }], publish? } 仅读 checksum+size   │
 │                                       │            upsert artifact     │
 │                                       │   publish=true → 置 published   │
 │  ◀──  { release_id, status, endpoints }│
```

设计要点：

- presign 接口按文件粒度签名，`expires` 短（5–10 min）；release 须已存在，presign 不自动建。
- 完整性靠 S3 原生 checksum：presign 绑 `x-amz-checksum-sha256`，PUT 回放该头，S3 收完自算 sha256 不符直接拒；complete 仅 `HeadObject` 读回 checksum + size 确认，**不二次下载**。
- complete 接口幂等：同 `upload_id` 重复调用返回相同 release（artifact 走 `(release_id, platform, target, arch, abi)` 唯一键 upsert）。
- `publish=true` 额外需 `release:publish`（缺 403）+ release ≥1 artifact → 置 `published`；developer（无 publish）跑 `publish=false` 留 draft。
- server 仅承担鉴权、scope 检查、metadata 写入；不走产物字节，单 binary 不被带宽拖累。
- 失败重试：CLI 持有 `upload_id` 与 `parts[]`，单文件 PUT 失败只重发该文件（`backon` 指数退避，仅重试 5xx/timeout/connect）。
- 不引 S3 multipart 客户端分片：composite checksum 与整体 sha256 强校冲突，MVP 单文件单 PUT。

### publish tauri

```bash
swarmhive publish tauri \
  --app swarmdrop \
  --version 0.4.5 \
  --channel stable \
  --artifacts ./src-tauri/target/release/bundle \
  --notes-file CHANGELOG.md
```

自动处理：

- 扫描 Tauri bundle 目录。
- 读取 latest.json。
- 识别 updater artifact 和安装包。
- 对每个产物计算 sha256 → 调 `/uploads/presign`。
- 按返回的 presigned URL 直传 S3 / RustFS / OSS（带进度条）。
- 调 `/uploads/{id}/complete` 提交 hash 与 ETag。
- 输出 endpoint。

### publish android

```bash
swarmhive publish android \
  --app swarmnote-rn \
  --version 0.2.1 \
  --version-code 21 \
  --channel stable \
  --apk ./android/app/build/outputs/apk/release/app-release.apk
```

自动处理：

- 读取 APK metadata。
- 校验 versionName / versionCode。
- 计算 APK sha256 → presign → 直传 → complete（同上）。
- 输出 Android check endpoint。

### 管理命令(与 Web Admin 对齐)

CLI 不只发布,还能管理 apps / channels / releases —— 与 Web Admin 同一组动作,走同一批 HTTP endpoint(`add-cli-management-commands`)。这样**用户可以让 AI 经 CLI / 脚本代为管理**。

```bash
# apps
swarmhive apps list
swarmhive apps get    --app swarmdrop
swarmhive apps create --slug swarmdrop --display-name SwarmDrop \
                      --platforms tauri-desktop,react-native-android
swarmhive apps update --app swarmdrop --display-name "SwarmDrop Pro"
swarmhive apps delete --app swarmdrop --yes        # 破坏性 → 需 --yes;有 release 时 409

# channels(promote / rollback 收编进此名词组)
swarmhive channels list        --app swarmdrop
swarmhive channels create      --app swarmdrop --name nightly
swarmhive channels set-default --app swarmdrop --name stable
swarmhive channels promote     --app swarmdrop --name stable --version 0.4.5
swarmhive channels rollback    --app swarmdrop --name stable          # 默认回退上一个
swarmhive channels rollback    --app swarmdrop --name stable --to-version 0.4.4

# releases(注意:releases publish ≠ publish tauri/android)
swarmhive releases list    --app swarmdrop
swarmhive releases get     --app swarmdrop --version 0.4.5
swarmhive releases create  --app swarmdrop --version 0.4.6 --notes-file CHANGELOG.md  # 建 draft,不上传
swarmhive releases update  --app swarmdrop --version 0.4.6 --android-version-code 41
# 灰度 / 强更策略(add-cli-release-policy,与 Admin UI parity):省略=不改,清空走显式 sentinel
swarmhive releases update  --app swarmdrop --version 0.4.6 --rollout-percent 50          # 灰度 50%
swarmhive releases update  --app swarmdrop --version 0.4.6 --min-version 0.4.0           # Tauri 强更下限
swarmhive releases update  --app swarmdrop --version 0.4.6 --android-min-version-code 41 # RN 强更下限
swarmhive releases update  --app swarmdrop --version 0.4.6 --rollout-percent 100 --min-version 0.0.0  # 取消灰度 + 移除下限
swarmhive releases publish --app swarmdrop --version 0.4.6   # 发布一个已存在的 draft
swarmhive releases yank    --app swarmdrop --version 0.4.5 --yes

# artifacts(只读)
swarmhive artifacts list --app swarmdrop --version 0.4.5
```

> `releases publish` 发布一个**已存在的 draft**(不上传);`publish tauri|android` 是「扫 bundle → 上传 → complete(默认发布)」的上传式发布。两者并存,按场景选。

### 配置命令:storage / mail(`add-cli-storage-mail-admin`)

配置线也对齐 Web Admin —— 用户可让 AI 帮忙「接上 OSS」「配好邀请邮件模板」。

```bash
# storage(init rustfs 仍是引导式一键;其余细粒度)
swarmhive storage list
swarmhive storage create --name minio --endpoint http://… --bucket b --region us-east-1 \
                         --access-key-id KEY --url-mode signed   # secret 见下
swarmhive storage update --backend minio --bucket newbucket     # 省略 secret = 保留
swarmhive storage test     --backend minio
swarmhive storage activate --backend minio
swarmhive storage cors     --backend minio --origin https://hive.example.com

# mail providers
swarmhive mail providers list
swarmhive mail providers create --name prod --host smtp.example.com --port 587 \
                                --encryption starttls --from-email no-reply@example.com   # password 见下
swarmhive mail providers activate --provider prod
swarmhive mail providers test     --provider prod
swarmhive mail providers delete   --provider prod --yes

# mail templates(多行正文走文件)+ logs + status
swarmhive mail templates list
swarmhive mail templates set --event user_invite --locale zh-CN \
                             --subject "欢迎" --html-file invite.html --text-file invite.txt
swarmhive mail templates preview --event user_invite --locale zh-CN --sample-file ctx.json
swarmhive mail templates restore-defaults
swarmhive mail logs --limit 50
swarmhive mail status
```

### 通知命令(`add-notifications-cli`)

通知管理也对齐 Web Admin —— provision-as-code / CI bootstrap 可在 CLI 完成。endpoint 用
`--endpoint <id|name>` 寻址;`whsec_` 签名密钥仅 `create` / `rotate-secret` 时打印一次。

```bash
# webhook endpoints(create / rotate-secret 一次性打印 whsec_)
swarmhive notifications endpoints list
swarmhive notifications endpoints create --name slack-releases --url https://hooks.slack.com/…
swarmhive notifications endpoints update --endpoint slack-releases --url https://… --disable
swarmhive notifications endpoints rotate-secret --endpoint slack-releases
swarmhive notifications endpoints test   --endpoint slack-releases      # 发 webhook.test,不入库
swarmhive notifications endpoints delete --endpoint slack-releases --yes

# subscriptions(event → email 地址 / webhook endpoint,可选 --app 限定单 app)
swarmhive notifications subscriptions list
swarmhive notifications subscriptions create --event release.published --channel email --to team@example.com
swarmhive notifications subscriptions create --event channel.promoted --channel webhook \
                                             --endpoint slack-releases --app swarmdrop
swarmhive notifications subscriptions delete --id <uuid> --yes

# deliveries(投递日志 + 详情 + 手动重投,redeliver 保持原 webhook-id)
swarmhive notifications deliveries list --endpoint slack-releases --status failed --limit 50
swarmhive notifications deliveries get --id <uuid> --output json   # 请求/响应快照(签名头+body)
swarmhive notifications deliveries redeliver --id <uuid>
```

**密钥三路输入**(S3 `access_key_secret` / SMTP `password`)—— 绝不进命令串:

```bash
# 1) env(推荐给 AI / CI)
SWARMHIVE_STORAGE_SECRET=… swarmhive storage create …
SWARMHIVE_MAIL_PASSWORD=…  swarmhive mail providers create …
# 2) stdin 管道
printf '%s' "$SECRET" | swarmhive storage create … --secret-stdin
# 3) 明文 flag(顺手,但会进 shell history / ps / 日志,AI / 脚本勿用)
swarmhive storage create … --access-key-secret …
swarmhive mail providers create … --password …
```

> 优先级 `--secret-stdin` > env > 明文 flag;有 TTY 且都没给则 create 时交互读入。`update` 三路都没给 = 保留已存 secret。

### 输出 / 错误契约(AI / skill 友好)

所有命令认全局 `--output {table|json}`:

- **成功** → `--output json` 时结果对象 / 数组打到 **stdout**(`apps create --output json` 给创建出的 App)。
- **失败** → API 错误解析成 RFC 9457 problem+json,`--output json` 时原样打到 **stderr**;本地错误(缺 `--yes`、缺凭证)打 `{"error": "..."}`。**任何失败 exit code 非零**。
- **全非交互**:token 走 `SWARMHIVE_TOKEN` env / `credentials.toml`,写操作全用 flag;破坏性操作要 `--yes`(不弹交互确认)。

> 配套 skill / AI 只需认「stdout=成功 JSON / stderr=problem JSON / exit code」这一套契约,就能稳稳驱动整个 CLI。给 AI 用时建议发一个**最小权限 API Token**(令牌页勾权限子集,如不含 `app:delete`),`--yes` + token 权限双保险。

## 用户体验要求

- 上传大文件必须有进度条。
- 所有发布命令支持 `--dry-run`。
- 错误信息要明确指出缺哪个文件、哪个签名、哪个字段。
- 发布成功后输出更新检查 URL 和下载 URL。
- CLI 输出应适合 CI 日志阅读。
- 支持 JSON 输出：`--output json`(成功 stdout / 错误 stderr problem+json / 非零 exit)。

## 与 Web Admin 的关系

分工：

- CLI / CI/CD 是**主**发布路径，适合自动化、批量、可重复发布。
- Web Admin 负责查看、配置、统计和排障，并提供**浏览器直传**作为补充：在版本的产物抽屉里拖拽上传，复用同一套 presign / complete 契约（浏览器内用 hash-wasm 流式算 MD5+SHA256、直传对象存储、可一键发布并 promote channel）。与 CLI `publish` 同源、行为一致。

这样既照顾不想接 CI 的手动发布场景，也保持 CLI 作为一等发布路径。浏览器直传需先为对象存储桶配置 CORS（见 [07-storage-and-delivery.md](07-storage-and-delivery.md) 的「浏览器直传与 CORS」）。

## 与 GitHub Action 的关系

官方 GitHub Action 应该尽量薄，只包装 CLI：

- 下载 CLI。
- 注入 server / token。
- 调用 verify。
- 调用 publish。

这样可以保证本地发布和 CI/CD 发布行为一致。

