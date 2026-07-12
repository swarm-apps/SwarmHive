## ADDED Requirements

### Requirement: RN Android 更新响应 SHALL 携带镜像候选

当有可用更新时,Android 更新检查响应 SHALL 在既有 `download_url`(主源间接入口)之外,新增可选
`mirror_urls: [..]`,列出该 artifact **已通过 liveness/digest 校验**的备用源(当前即 GitHub
Release)。每个 URL SHALL 走 `/download/{app}/{version}/{artifact_id}?source=…` 间接层(非
`github.com` 直链),以保留 `download_intent` 埋点与 liveness gating。无备用源或备用源未通过校验时,
`mirror_urls` SHALL 为空数组。该字段为纯增量,不改变既有 `has_update` / `download_url` /
`sha256` / 版本闸门语义。

#### Scenario: 有可用更新且备用源已校验

- **GIVEN** 一个已发布 RN release,其 artifact 有一个已通过校验的 GitHub `mirror_url`
- **WHEN** 客户端检查更新且判定有更新
- **THEN** 响应 `has_update=true`、`download_url` 指向主源间接入口
- **AND** `mirror_urls` 含该 artifact 的 `?source=github` 间接 URL

#### Scenario: 无备用源时字段为空

- **GIVEN** 一个 artifact 只有 S3、无 `mirror_url`(或镜像未通过校验)
- **WHEN** 客户端检查到有更新
- **THEN** 响应照常返回 `download_url`,且 `mirror_urls` 为空数组
