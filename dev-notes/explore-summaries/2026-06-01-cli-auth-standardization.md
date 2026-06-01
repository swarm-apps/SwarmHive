# Explore Summary — CLI 登录规范化 + OAuth 收口（2026-06-01）

> explore 模式产出的临时决策档，给 `/opsx:propose add-cli-device-login` 引用。归档前不要 commit 到 `dev-notes/knowledge/`——决策落到 proposal + 实现后，知识沉淀进 `dev-notes/knowledge/backend.md`，本文件可删。与 [2026-05-27-account-onboarding.md](2026-05-27-account-onboarding.md) / [2026-05-28-upload-and-cli-stack.md](2026-05-28-upload-and-cli-stack.md) 同款临时档。

## 触发问题

用户：「接下来该干什么，我觉得可以继续完成登录流程支持 OAuth，但我觉得当前 CLI 的登录不是很规范，去调研主流方案。」

两条线其实是同一方向的两端：**把认证从『客户端直接拿密码』升级成『浏览器委托授权』**。

## 现状诊断：CLI 登录是 ROPC 反模式

`swarmhive login`（[login.rs](../../crates/swarmhive-cli/src/commands/login.rs)）收集明文 email+password → `POST /api/v1/auth/cli-token`（[routes/auth.rs::cli_token](../../crates/swarmhive-server/src/routes/auth.rs)）→ `verify_password` → 铸永不过期 PAT 存 `credentials.toml`。这是 OAuth 2.1 / Security BCP（RFC 9700）明确废弃的 **Resource Owner Password Credentials**。实际后果：

1. 客户端经手主密码（泄露的不是可撤销 token）。
2. **与正在做的 web OAuth 直接对撞**——`add-oauth-github-and-provider-config` 上线后，OAuth-only 用户没密码可填，被锁死在 CLI 外。
3. 与未来 MFA 互斥。

## 调研：主流 CLI 怎么做

没有人再用密码授权，业界两条委托浏览器的成熟路线：

| 工具 | 主路径 |
|---|---|
| `gh`（最像 SwarmHive 的兄弟） | **Device Flow（RFC 8628）**：显示 user_code，去浏览器输入 |
| `gcloud` | Loopback + PKCE：起本地端口，浏览器回跳 127.0.0.1 |
| `vercel` | Loopback + PKCE，headless 退化 magic link |
| `stripe` | Device code + workspace 选择 |
| `aws` (v2.22+) | 从 device 切回 PKCE 默认（防钓鱼） |

WorkOS 决策框架：**笔记本默认 PKCE，受限环境（SSH/容器/Cloud IDE）退 device**。loopback 在 SSH 进远程机即失效（浏览器够不到远程机 127.0.0.1）；device 只需出站 HTTPS，全环境可用，但有已知钓鱼模式。

来源：RFC 8628 · cli/oauth（gh 的 Go 库）· WorkOS「PKCE vs Device Flow」「Browser-based OAuth into your CLI」。

## 关键架构洞察：SwarmHive 既是 AS 又是 RP

CLI 只跟 SwarmHive 说话，**不需要认识 GitHub**。人怎么证明身份，是 SwarmHive 在浏览器 `/login` 页决定的（密码 或 "Sign in with GitHub"）。于是 CLU 委托登录的浏览器步骤**复用 web `/login`**：

- OAuth-only 用户自动获得 CLI 能力（CLI 零 GitHub 代码）
- 第 2 点对撞问题消失
- 未来 MFA/SSO/密码策略 CLI 全白嫖（认证逻辑只有一处）

服务端积木现成：tower-sessions（批准页一键批准）+ `token_service::create`（铸 PAT）。新增面只有 device 端点 + 一个 SPA 批准页 + CLI 轮询。

## 拍板（用户 2026-06-01 决策）

| 决策点 | 选定 | 理由 |
|---|---|---|
| CLI 主路径 | **Device Flow 默认（gh 风格）** | release CLI 大量在远程 build 机/SSH/CI/容器跑、server 常内网自托管；loopback 在这些环境失效。device 全环境可用、不起本地端口。钓鱼风险在单组织自托管场景极低。 |
| ROPC `cli-token` | **直接废弃替换** | CI 非交互本就用 Web Admin 创的 scoped API Token + `SWARMHIVE_TOKEN`（已实现），不依赖密码授权。 |
| 落地范围 | 两个 proposal 一起规划 | 新 `add-cli-device-login` + 给现有 `add-oauth-github-and-provider-config` 补交叉引用。 |

明确**不做**（留后续）：loopback+PKCE（`add-cli-loopback-login`，`--web` 可选加速）· OS keychain token 存储（`add-cli-credential-keychain`）· 短时 token+refresh · 多 client_id 注册。

## 两 proposal 接口契约（唯一一条）

`add-cli-device-login` 的 `/device` 批准页做成 **public 顶层路由**（不放 `_auth/`——auth guard 的 `next: location.pathname` 会丢 `?user_code`）。未登录时引导 `/login?next=<encode 完整 path+search>`，登录后带回 user_code。device 页由此**自动继承 `/login` 上所有登录方式**（密码 → 未来 GitHub）。两 proposal **服务端零代码耦合**，仅共享 `/login` 闸门，可任意顺序落地。

## RFC 8628 实现要点（落到 design.md）

- `client_id = "swarmhive-cli"`（public client，无 secret）；`grant_type = urn:ietf:params:oauth:grant-type:device_code`。
- token 端点用 **RFC 8628 wire 格式** `400 { error }`（`authorization_pending`/`slow_down`/`access_denied`/`expired_token`/`invalid_grant`），破例不走仓库 RFC 9457；其余人面向端点仍 RFC 9457。
- `device_code`(32B)→blake3 唯一存储；`user_code` 8×base20 `WDJB-MJHT`，活跃集合内唯一（**不**装 partial unique index，rc.38 schema-sync bug），15min TTL + lazy 清理。
- token 铸造复刻 `cli_token` 临时 Principal + `token_service::create(Pat)`；铸成置 `completed`，二次轮询 `invalid_grant`。
- bootstrap window（user 表空）`/device/code` → 410 typed `device_not_available_during_bootstrap`（对称 OAuth 排除）。
