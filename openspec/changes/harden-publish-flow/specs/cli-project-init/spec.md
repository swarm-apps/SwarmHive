## ADDED Requirements

### Requirement: init 可一步打通 CI token 与 workflow 样板

`swarmhive init` SHALL 支持 `--setup-ci-token`,在生成 `swarmhive.toml` 之后,引导创建一个带 `ci-publish` 预设权限的 CI token,并打印把它写入 GitHub secret 的命令(`gh secret set SWARMHIVE_TOKEN`),同时生成一份可直接 copy-paste 的 release.yml 样板。在 `--json`(非交互/AI/CI)模式下,该命令 MUST 以结构化字段输出建议命令、secret 名与样板路径,不得有交互提示。

#### Scenario: setup-ci-token 打通接入第一步
- **WHEN** 运行 `swarmhive init --setup-ci-token`
- **THEN** 生成 `swarmhive.toml`、创建含 `release:update` 的 CI token、打印 `gh secret set SWARMHIVE_TOKEN ...`、并产出一份可用的 release.yml 样板

#### Scenario: json 模式无交互且字段完整
- **WHEN** 以 `swarmhive init --setup-ci-token --json` 运行(非 TTY)
- **THEN** 输出 MUST 为单个 JSON 对象,包含建议的 token 创建命令、`SWARMHIVE_TOKEN` secret 名与建议的 workflow 路径,且 MUST 不阻塞等待输入
