## ADDED Requirements

### Requirement: App 详情页提供常驻上下文外壳

The admin SPA SHALL provide an App detail route `/apps/:slug` rendered as a `PageContainer` whose header constantly shows the app's display name, slug, and platform tags — so the "which app am I in" context never scrolls out of view — plus a local breadcrumb `应用 / <slug> / <tab>`. The shell SHALL load the app via `GET /api/v1/apps/:slug` in `beforeLoad`; a missing app (404) SHALL redirect to `/apps`. The route SHALL live inside the `_auth` guard.

#### Scenario: 进入详情页显示常驻 app 上下文

- **GIVEN** an authenticated session and app `swarmdrop` exists
- **WHEN** the user navigates to `/apps/swarmdrop`
- **THEN** the page header shows `swarmdrop`'s display name, slug, and platform tags
- **AND** a breadcrumb `应用 / swarmdrop / 版本` is shown

#### Scenario: 不存在的 app 兜底回列表

- **WHEN** the user navigates to `/apps/does-not-exist`
- **THEN** the app load fails and the user is redirected to `/apps`

### Requirement: App 详情以版本/渠道 tab 组织且走子路由

The detail shell SHALL present a tab list `版本 / 渠道` whose active tab is derived from the current pathname via `useRouterState` (not a non-reactive router snapshot). Each tab SHALL be a child route — `/apps/:slug/releases` and `/apps/:slug/channels` — so deep links resolve directly without first visiting another tab. Visiting `/apps/:slug` SHALL redirect to the version tab by default.

#### Scenario: 默认落到版本 tab

- **WHEN** the user navigates to `/apps/swarmdrop`
- **THEN** the user is redirected to `/apps/swarmdrop/releases` with the 版本 tab active

#### Scenario: 渠道 tab 深链接可直达

- **WHEN** the user opens `/apps/swarmdrop/channels` directly
- **THEN** the 渠道 tab renders active without first visiting the 版本 tab

### Requirement: App 编辑与删除从详情页头触发

The app edit (`PATCH /api/v1/apps/:slug`) and delete (`DELETE /api/v1/apps/:slug`) affordances SHALL live in the detail page header actions, moved off the `/apps` list rows, gated on `app:update` and `app:delete` respectively. The existing field set (display name, platforms), the slug-immutability, and the app-has-releases `409` handling SHALL be preserved unchanged.

#### Scenario: 从详情页头编辑 app

- **GIVEN** an Owner viewing `/apps/swarmdrop`
- **WHEN** the Owner opens edit from the header and changes the display name
- **THEN** the request PATCHes `/api/v1/apps/swarmdrop`
- **AND** the header reflects the new name after refetch

#### Scenario: 删除有版本的 app 被阻止

- **GIVEN** an Owner viewing `/apps/swarmdrop` which has at least one release
- **WHEN** the Owner confirms delete from the header
- **THEN** a message states the app still has releases and the user stays on the detail page

### Requirement: 渠道 tab 统一承载 channel 管理与发布列车

The 渠道 tab SHALL combine channel configuration (list / create / set-default via `/api/v1/apps/:slug/channels`) with the release-train pointer view (each channel's current release plus promote / rollback), so a channel's configuration and its current pointer are managed in one place rather than split across the apps list and a separate releases page.

#### Scenario: 渠道 tab 同时展示 channel 列表与当前指针

- **GIVEN** app `swarmdrop` with channels `stable` and `beta`
- **WHEN** the user opens the 渠道 tab
- **THEN** each channel shows whether it is default and the release it currently points at
- **AND** create-channel, set-default, and promote / rollback affordances are present (permission-gated)
