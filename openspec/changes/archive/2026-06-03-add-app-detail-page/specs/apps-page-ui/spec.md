## MODIFIED Requirements

### Requirement: Admin SHALL list applications in a table

The `/apps` page SHALL render a `ProTable` whose rows come from `GET /api/v1/apps`, showing display name, slug, platforms, and created-at. Each row SHALL provide a navigation affordance into that app's detail page (`/apps/:slug`); entering the detail is the primary way to reach the app's releases and channels. (The default channel is not a list column — the `App` resource does not carry it; it is shown and managed in the per-app 渠道 tab.) The page SHALL be reachable only inside the `_auth` guard (authenticated). When the API returns an empty list the table SHALL render an empty state, not an error.

#### Scenario: Authenticated user sees existing apps

- **GIVEN** an authenticated session and two apps exist
- **WHEN** the user navigates to `/apps`
- **THEN** the table renders two rows with each app's slug and platforms
- **AND** each row offers a way to open that app's detail page

#### Scenario: Entering an app opens its detail

- **GIVEN** the `/apps` list with app `swarmdrop`
- **WHEN** the user activates the row's enter-detail affordance for `swarmdrop`
- **THEN** the app navigates to `/apps/swarmdrop`

#### Scenario: Empty list renders empty state

- **GIVEN** an authenticated session and zero apps
- **WHEN** the user navigates to `/apps`
- **THEN** the table renders an empty state and no error

## REMOVED Requirements

### Requirement: Admin SHALL edit an application

**Reason**: 编辑入口从 `/apps` 列表行迁移到 App 详情页头（见 `app-detail-navigation`），消除列表行的操作拥挤。编辑的字段集（display name / platforms）、slug 不可变、409 处理均不变，仅触发位置改变。
**Migration**: 在 `/apps/:slug` 详情页头打开编辑。

### Requirement: Admin SHALL delete an application and surface the has-releases block

**Reason**: 删除入口同样迁移到 App 详情页头（见 `app-detail-navigation`）；app-has-releases `409` 阻止行为不变，仅触发位置改变。
**Migration**: 在 `/apps/:slug` 详情页头执行删除。

### Requirement: Admin SHALL manage an application's channels

**Reason**: channel 管理（列表 / 创建 / 设默认）从 `/apps` 行内 Drawer 迁移到 App 详情的「渠道 tab」，并与发布列车指针（promote / rollback）合并到一处（见 `app-detail-navigation` 与 `releases-page-ui`），消除 channel 配置与指针分散两页的割裂。底层 endpoint 与行为不变。
**Migration**: 在 `/apps/:slug/channels` 渠道 tab 管理 channel。
