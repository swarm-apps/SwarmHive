# Tasks — add-self-service-account

## 1. 后端:共享 helper 提升

- [x] 1.1 [code] 把 `routes/password_reset.rs` 的 `upsert_credentials` / `revoke_user_sessions` 提升到 `auth/service.rs`(`pub(crate)`,泛型 `C: ConnectionTrait`),原处改为调用提升后的函数。

## 2. 后端:api-types DTO

- [x] 2.1 [code] `crates/swarmhive-api-types/src/user.rs` 新增 `UpdateMeReq { display_name: String }` + `ChangePasswordReq { current_password: Option<String>, new_password: String }`(serde + `utoipa::ToSchema`,字段 example/description 英文)。

## 3. 后端:routes/account.rs

- [x] 3.1 [code] 新建 vertical-slice `routes/account.rs`,`router()` 暴露 `routes!(update_me)` + `routes!(change_password)`。
- [x] 3.2 [code] `PATCH /api/v1/users/me`:`Principal`(Active)→ load user → trim + 校验 `display_name` 1..=100(空/超长 → 422)→ `user.save` → 返回 `api::User`。
- [x] 3.3 [code] `PUT /api/v1/users/me/password`:credential 存在性分支 + `current_password` 校验(422 typed `current_password_incorrect`)+ `validate_strong_password`(422 `password_too_weak`)+ TX(upsert_credentials + revoke_user_sessions)+ `establish_session` 重发当前 + audit_log + 204。
- [x] 3.4 [code] `lib.rs` 的 `sensitive_routes()` merge `routes::account::router()`(单一来源,openapi_router 自动继承);`routes/mod.rs` 加 `pub mod account;`。
- [x] 3.5 [code] 新增 typed problem `current-password-incorrect`(422),走 `ApiError::typed`。

## 4. OpenAPI codegen

- [x] 4.1 [code] `pnpm openapi` regen `apps/admin/src/lib/api/schema.gen.ts`,`git add`。确认 operationId `update_me` / `change_password` 全局唯一(避开 mail/oauth 同名)。

## 5. 前端:API helper

- [x] 5.1 [code] `lib/api/account.ts` 新增 `patchDisplayName(name)` + `changePassword(current, next)`(`fetchClient` imperative 范式);导出 typed error 常量 `ERR_CURRENT_PASSWORD_INCORRECT`。

## 6. 前端:/profile 合并

- [x] 6.1 [code] `_auth/profile.tsx` 改 `PageContainer` + `tabList`(账户信息 / 安全 / 登录方式)。账户信息:邮箱(只读)+ 显示名(可编辑,提交 `patchDisplayName` + invalidate me)+ 验证状态/重发(从 account.tsx 迁入)。安全:改/设密码表单(current 条件 + new + 确认,提交 `changePassword`,成功 notification + 提示「其它设备已登出」)。登录方式:保留既有 OAuth 列表/绑定/解绑。
- [x] 6.2 [code] 删除 `_auth/settings/account.tsx`。
- [x] 6.3 [code] `_auth/route.tsx`:`settingsRoute` 改为 `canManageSettings ? [...] : []`(去掉「账户」子项 + 去掉 everyone 例外),更新过时注释。
- [x] 6.4 [code] `_auth/settings/index.tsx`:beforeLoad 改为依 `me.permissions` 重定向到首个 enabled manage 模块(mail→auth→storage 顺序),无任一 manage 权限则 `redirect /profile`。
- [x] 6.5 [code] `lingui:extract` 更新 `zh-CN/messages.po`,`git add`。

## 7. 测试

- [x] 7.1 [test] `crates/swarmhive-server/tests/account_smoke.rs`:design.md 第 4 节四组断言(改名持久化 / current 校验 / 弱密拒绝 / 改密踢 session / OAuth-only 设密)。
- [x] 7.2 [test] 复跑 `cargo test --workspace`(确认提升 helper 未破坏 password_reset 路径)。

## 8. docs / memory 同步

- [x] 8.1 [docs] `docs/13-rbac.md` 加「Self-service account」段(个人 vs 组织设置可达性分层 + self 端点鉴权模型)。
- [x] 8.2 [docs] `docs/08-admin-and-analytics.md` 更新 Profile 页职责(吸收账户信息 + 安全)。
- [x] 8.3 [docs] 更新 `dev-notes/knowledge/{backend,admin-spa}.md`:self-service 端点鉴权范式 + IA 分层(个人=头像下拉、设置=组织级 manage 门控)。
- [x] 8.4 [docs] `openspec/changes/README.md` 状态表 + 依赖图加本 change。

## 9. 质量门

- [x] 9.1 `cargo fmt --all` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`。
- [x] 9.2 `pnpm lint` + `pnpm --filter @swarm-hive/admin typecheck` + `pnpm admin:build`。

## 实施备注（apply 偏差）

- **2.1 DTO 落点改 `routes/account.rs` 而非 api-types**：`UpdateMeReq` / `ChangePasswordReq` 只被 account 单个 route 用、CLI 不消费，按「单 route DTO 留 route 文件」约定（同 `ResetPasswordReq`），定义在 `routes/account.rs`（带 `garde::Validate` + `ToSchema`，仍进 openapi → schema.gen.ts）。比放 api-types 更合规、不膨胀共享 crate。
- **新增 `MeResponse.has_password: bool`（design.md 原写「不加」的反转）**：前端「安全」tab 必须据此决定「改密码 vs 设密码」表单分支，正是 oauth change 里「待真有 UI 分支需求再加」的兑现点。加在 `MeResponse`（`/auth/me`）而非纯 `User` DTO（保持 User 纯净）；后端用 `count > 0` 不读 argon2 hash。
- **审计 action 名用 `password_changed`（无 `auth:` 前缀）**：对齐既有 `password_reset_completed` / `email_verified` 等无前缀命名，spec 里写的 `auth:password_changed` 仅示意。

## 对抗式审查修复（multi-agent review 后）

- **[medium] Bearer/PAT 改密的孤儿 session**：`change_password` 原无条件 `establish_session`——Bearer/PAT 调用（无 cookie）会在 `revoke_user_sessions` 删全部 session **之后**凭空 insert 一行新 session + 下发无人消费的 Set-Cookie，破坏「改密=其它会话全失效」。修：`establish_session` 仅在 `matches!(principal.auth_method, AuthMethod::Session{..})` 时调用；Bearer/PAT 跳过（其它会话已全删、不产生孤儿）。新增回归测试 `bearer_change_password_revokes_all_without_orphan_session`（PAT 改密后该用户 session 数=0、无 Set-Cookie）。
- **[low] 改密表单明文残留**：`SecurityTab` 的 ProForm `onFinish` 返回 true 不会 `resetFields`（那只对 ModalForm/DrawerForm 控制弹窗有意义），明文密码残留在表单态/DOM。修：加 `formRef` + 成功分支 `formRef.current?.resetFields()`。
- 另 2 条原始发现经对抗式验证被证伪（非真实缺陷）。
