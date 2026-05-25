# SwarmHive 竞品分析

> 调研日期：2026-05-22
> 调研范围：Tauri 桌面端更新分发、React Native / Expo OTA、通用开源自托管更新平台、国内 APK 分发。

## 一、市场三层格局

| 层 | 角色 | 代表 | 与 SwarmHive 关系 |
|---|---|---|---|
| 协议层 | 客户端 SDK，定义更新协议，不提供服务端 | Tauri Updater、Sparkle、Squirrel | SwarmHive 必须兼容，互补不竞争 |
| 托管 SaaS 层 | 闭源、按量计费、含 CDN/证书托管 | CrabNebula Cloud、ToDesktop、Expo EAS、Stallion、Codemagic CodePush | 价值观对立，SwarmHive 是自托管开源平替 |
| 自托管层 | 自部署、自管存储 | UpgradeLink、faynoSync、Capgo、Hot-Updater、electron-release-server、Nucleus、Hazel/Nuts | 真正的赛场，竞争都在这里 |

## 二、威胁度分级

### Tier S：直接竞品（功能重叠 ≥ 60%）

#### 1. UpgradeLink — 最大威胁

- 仓库：[toolsetlink/upgradelink](https://github.com/toolsetlink/upgradelink)
- 官网：[toolsetlink.com](https://www.toolsetlink.com/)
- 国内 Go 项目，开源 + 托管站。Tauri + Android APK + Electron 三端几乎与 SwarmHive 完全重叠。
- 短板：Go 技术栈、无 S3 抽象、RBAC 薄弱（接近单 admin）、无 SDK + registry 分发形态。
- 用户心智：国内开发者第一反应就是 UpgradeLink。SwarmHive 必须给出"为什么换"。

#### 2. faynoSync — 最强活跃自托管对手

- 仓库：[ku9nov/faynoSync](https://github.com/ku9nov/faynoSync)
- Go + MongoDB + Redis，37 个 release，2026 年 5 月仍在更新。
- 已经做到：S3/MinIO/Garage/GCS/DO Spaces 全覆盖、JWT + API Key、TUF 签名、单独 dashboard、Tauri/Electron 支持。
- 短板：无 React Native Android、技术栈重（Mongo + Redis）、无单机 bundled 存储模式、无国内分发优化。

#### 3. CrabNebula Cloud — SaaS 头号假想敌

- 官网：[crabnebula.dev/cloud](https://crabnebula.dev/cloud/)
- Tauri 官方合作伙伴，€5/10k 下载量；闭源 SaaS。
- 国外用户的"标准答案"。SwarmHive 是其"自托管开源平替"位。

#### 4. Capgo — 哲学最相似的项目

- 官网：[capgo.app](https://capgo.app/)
- 自托管、MIT、S3 后端、CLI、Web 后台、单组织友好，自我定位"Appflow 替代品"。
- 关键差异：Capgo 是 Capacitor + JS OTA，SwarmHive 是 Tauri/RN + 全量包。当前不重叠，但如果 SwarmHive 未来做 OTA provider，Capgo 是直接参考也是潜在对手。

### Tier A：相邻竞品（重叠 30–60%）

| 产品 | 范围 | 重叠点 | 不重叠点 |
|---|---|---|---|
| [Hot-Updater](https://github.com/gronxb/hot-updater) | RN JS OTA | 自托管 + S3 + CLI + 后台 | 不做 APK / Tauri |
| [Atlassian Nucleus](https://github.com/atlassian/nucleus) | Electron 多通道 | 自托管 + S3 + CLI + 通道 | 仅 Electron，2018 后停滞 |
| [electron-release-server](https://github.com/ArekSredzki/electron-release-server) | Electron 全套 | 自托管 + 后台 + LDAP | 仅 Electron，AngularJS，2023 后慢维护 |
| [Velopack](https://velopack.io/) | Win/macOS/Linux 打包+客户端 | Rust、跨平台、增量包 | 无服务端，纯客户端 |
| [蒲公英 Pgyer](https://www.pgyer.com/) | 国内 APK/IPA 分发 | APK + 国内 + 权限 | 闭源、无 Tauri、无 CLI-first |
| [Xavia OTA](https://github.com/xavia-io/xavia-ota) | Expo 协议 OTA | 自托管 + S3 | OTA-only，无 APK / Tauri |
| [React Native Stallion](https://stalliontech.io/) | RN OTA 企业版 | 自托管 + S3 + CDN | RN-OTA-only，闭源 |
| [ToDesktop](https://www.todesktop.com/) | Electron BaaS | CLI + 后台 + CI | 闭源 SaaS，仅 Electron |
| [Hazel](https://github.com/vercel/hazel) / [Nuts](https://github.com/GitbookIO/nuts) | Electron 代理 | 思路类似 | 依赖 GitHub Releases，国内不可用 |

### Tier B：已死或退化（用作市场空白证据）

- Microsoft App Center：2025-03 已关停 → 整个市场重新洗牌的导火索。
- Microsoft CodePush server：2025 archived → Hot-Updater / Capgo / Stallion 接盘。
- Hazel / Nuts：基本停滞，且都依赖 GitHub Releases → 国内不可用。
- Squirrel.Windows：自 2019 未维护。
- Fir.im：实际已收缩为 iOS 签名服务。
- google/omaha：2025-08 archived，只能作为协议参考。

## 三、SwarmHive 的差异化卡位（Wedge）

把所有竞品扔到坐标系里，没有任何产品同时勾选下面九项：

| 维度 | SwarmHive | UpgradeLink | faynoSync | CrabNebula | Capgo | Hot-Updater |
|---|---|---|---|---|---|---|
| 开源自托管 | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ |
| Tauri 全量更新 | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| RN Android APK | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| S3-compatible 抽象 | ✅ | ❌ | ✅ | ❌ | ✅ | ✅ |
| bundled 单机存储（RustFS） | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 真正 RBAC（多角色） | ✅ | ❌ | 仅 per-user | ❌ | ❌ | ❌ |
| shadcn registry 分发 UI | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| 国内分发优化（OSS preset） | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Rust 单体二进制 | ✅ | ❌ Go | ❌ Go | — | — | — |

**独家卡位三件套**：

1. **Tauri + RN-Android 同栈** —— 唯一同时覆盖这两端的自托管产品。
2. **bundled RustFS 单机模式** —— 单服务器用户开箱即用，UpgradeLink / faynoSync 都没做。
3. **shadcn registry 双 UI 包**（registry-web + registry-rn）—— 前端生态独此一家。

## 四、给 SwarmHive 的建议

### 1. README 与 landing page 必须直接对标 UpgradeLink 与 faynoSync

不要泛泛说"自托管更新平台"，而是给出明确对照表：

- 比 UpgradeLink 多了什么：S3 抽象、bundled RustFS、真 RBAC、UI 组件 registry。
- 比 faynoSync 多了什么：RN-Android、单 binary 部署、Rust 全栈、国内分发优化。

### 2. OTA provider 不能拖太久

Hot-Updater + Capgo 正在快速吃掉 RN/Capacitor 市场，App Center 关停的红利窗口大约 18–24 个月。

- Expo Updates 协议是事实标准，Xavia / Hot-Updater 都已实现。
- 应作为首个 OTA provider 候选，复用现有协议而不是从零重写。

### 3. 国内场景叙事是最强护城河

CrabNebula、Capgo、Hot-Updater、faynoSync 都没认真处理"GitHub 慢 + 阿里云 OSS + 国内备案"这套组合。

- 官网中文文档应优先于英文文档写完整。
- UpgradeLink 在这条线上是先发，但工程现代化程度落后于 SwarmHive 的设计。

## 五、需要持续关注的对手

- **faynoSync**：每月迭代，技术栈与 SwarmHive 距离最近，应订阅其 release 跟踪 feature parity。
- **UpgradeLink**：国内心智先发，应关注其 RBAC、SDK、Tauri v2 兼容性进展。
- **Hot-Updater**：Vercel OSS Program 成员，社区扩张快；OTA provider 设计需要参考其 plugin 模型。
- **Capgo**：自托管 OSS 哲学的标杆，pricing 与文档结构均值得参考。

## 附录：调研来源摘录

- App Center retirement：[Embrace 博客](https://embrace.io/blog/app-center-retirement/) / [Infinite Red](https://shift.infinite.red/microsoft-is-retiring-app-center-heres-what-react-native-developers-should-use-instead-c2a8786f971e)
- Expo EAS Update pricing：[expo.dev/pricing](https://expo.dev/pricing)
- China app store 现状：[澎湃新闻](https://www.thepaper.cn/newsDetail_forward_17260093) / [知乎 上架指南](https://zhuanlan.zhihu.com/p/665885019)
- Tauri Updater 协议：[v2.tauri.app/plugin/updater](https://v2.tauri.app/plugin/updater/)
- Self-Hosting Expo OTA 案例：[jmensah.hashnode.dev](https://jmensah.hashnode.dev/how-i-built-a-multi-app-ota-update-system-and-cut-costs-from-199-month-to-0)
