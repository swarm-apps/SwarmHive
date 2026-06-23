# add-cli-telemetry

## Why

server 暴露 5 个 telemetry 查询端点(`/api/v1/telemetry/{overview,summary,adoption,funnel,distribution}`,`telemetry:read` 门控,`add-telemetry-events` + `add-dashboard-overview`),Admin 有 `/telemetry` 页 + 首页 dashboard。但 **CLI 零覆盖**——CI / DevOps 发布后想验证采用率、下载漏斗、版本分布只能开 Web Admin,无法纯 CLI / 脚本拉数据。补 CLI 查询命令。

## What

- **`commands/telemetry.rs`(新)**:5 个只读查询,消费既有端点(零 server / api-types 改;telemetry DTO 早在 api-types):
  - `telemetry overview [--days N]` → 全局速览(`TelemetryOverview`,`emit_one`)。
  - `telemetry summary --app <slug> [--days N]` → 指标卡(`TelemetrySummary`,`emit_one`)。
  - `telemetry adoption --app <slug> [--days N]` → 版本采用(`Vec<AdoptionPoint>`,`emit`)。
  - `telemetry funnel --app <slug> [--days N]` → 更新漏斗(`Vec<FunnelStage>`,`emit`)。
  - `telemetry distribution --app <slug> [--days N] --dim <platform|arch|version|channel>` → 分布(`Vec<DistributionSlice>`,`emit`)。
  - `--days` clap `default_value_t = 30`(server 同默认);query 走 `format!` 拼串(同 mail logs `?limit=`)。
- **`main.rs`**:`Command::Telemetry { TelemetryCommand }` 子命令枚举 + dispatch。
- 各命令定义 `*Row`(tabled)给 table 输出;`--output json` 走 DTO 原样 pretty。

## Acceptance

- `cargo build -p swarmhive-cli`;`clippy --workspace --all-targets -- -D warnings`;`fmt --check`。
- `swarmhive telemetry --help` 列出 5 子命令;`telemetry overview --help` 等显示 flag。
- `cargo test -p swarmhive-cli`;`cargo tree -p swarmhive-cli | grep -E "sea-orm|swarmhive-entity"` 仍空。

## Non-goals

- **零 server / 零 api-types 改动**——端点与 DTO 已就绪。
- **只读查询**,不做 telemetry 写入(`POST /events` 是 SDK 上报契约,非 CLI 职责)。
- **不**做本地缓存 / 趋势图渲染;table + json 两种输出,趋势细节走 `--output json`。
- CLI-binary e2e deferred(同既有 CLI 命令——bin crate 不可 import + 需真实 server;端点行为由 `telemetry_smoke` 覆盖)。

## Depends on

`add-telemetry-events`(summary/adoption/funnel/distribution 端点)+ `add-dashboard-overview`(overview 端点)+ `add-cli-management-commands`(CLI client/emit 基建)——均已归档。

## Maps to docs

- `docs/12-cli.md`(新 telemetry 命令段)
- `docs/10-telemetry.md`(查询口径)
