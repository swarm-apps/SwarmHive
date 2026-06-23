# Tasks — add-cli-release-policy

## 1. clap flag

- [x] 1.1 [code] `main.rs` `ReleasesCommand::Update` 加 `--min-version` / `--rollout-percent` / `--android-min-version-code`(`Option`,英文 help 注明 sentinel 清空)。
- [x] 1.2 [code] `main.rs` `ReleasesCommand::Create` 加 `--android-min-version-code`;两处 dispatch 透传。

## 2. command 透传

- [x] 2.1 [code] `commands/releases.rs::update` 签名加 3 参数 → 填 `UpdateReleaseRequest`(flag 直接映射 `Option`,省略=不改)。
- [x] 2.2 [code] `commands/releases.rs::create` 签名加 `android_min_version_code` → 填 `CreateReleaseRequest`(去掉硬编码 None)。

## 3. 表格展示

- [x] 3.1 [code] `ReleaseRow` 加 `rollout` / `min ver` 列(`rollout_percent ?? 100`、`min_version` 去 `0.0.0` sentinel 显示);`release_row` 填充。

## 4. Gates + Docs

- [x] 4.1 [test] `cargo build/clippy --workspace --all-targets -D warnings/fmt --check`;`releases {update,create} --help` smoke;`cargo test -p swarmhive-cli`;`cargo tree` 无 sea-orm/entity。
- [x] 4.2 [docs] `docs/12-cli.md` releases 段补 policy flag;`dev-notes/knowledge/backend.md` CLI 段记「CLI 清空走显式 sentinel(vs UI compare-to-initial)」。
- [x] 4.3 [docs] `openspec/changes/README.md` 状态表加本 change。

## 5. 审查 + 归档

- [x] 5.1 [chore] 对抗式审查(11 项核查)→ 仅 1 finding:`--rollout-percent` help 称「1-100」但 clap 只校验 i16 类型范围(0/101 透到服务端才 422)。采纳:加 `value_parser = value_parser!(i16).range(1..=100)`,CLI parse 即拒(0/101 实测被拒、50 过),help 名副其实 + 省一次服务端往返。其余 10 项 OK(字段集 / dispatch 顺序 / Option 透传无漂移 / sentinel 展示 / 边界无 entity 泄漏)。
- [ ] 5.2 [chore] commit(feat)+ `openspec archive` + commit(chore)。
