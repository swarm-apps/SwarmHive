## MODIFIED Requirements

### Requirement: RN Android 更新响应 SHALL 携带镜像候选

当有可用更新时,Android 更新检查响应 SHALL 在既有 `download_url`(主源间接入口)之外,携带
`mirror_urls: [..]`,列出该 artifact **主源之外**的其余可用源,**按 fallback 顺序排列**。主源
由该 app 对该 artifact platform 的偏好(`github_source.prefer_for_platforms`)决定,并由裸
`download_url` 的 302 解析兑现;因此 `mirror_urls` SHALL NOT 包含主源已解析到的那个源 ——
否则客户端会把同一个投递位置试两遍。

- 未配偏好(缺省)时,主源为 OSS,`mirror_urls` 即**已通过 liveness/digest 校验**的 GitHub
  `?source=github` 候选 —— 与本 change 前语义**逐字节一致**。
- 偏好 GitHub 且镜像已校验时,主源为 GitHub,`mirror_urls` 为 `?source=oss` 候选(该 artifact
  有 S3 对象且存在活跃 backend 时)。

每个 URL SHALL 走 `/download/{app}/{version}/{artifact_id}?source=…` 间接层(非 `github.com`
直链),以保留 `download_intent` 埋点与 liveness gating。无其余可用源时该字段 SHALL 在响应中
**省略**(既有 `skip_serializing_if = "Vec::is_empty"`,线上不会出现 `[]`;SDK 侧 `?? undefined`
兜底)—— 本 change 沿用该序列化行为,不改。该字段不改变既有 `has_update` / `download_url` /
`sha256` / 版本闸门语义 —— `download_url` 的**形状**恒为裸间接入口,只有其 302 目标随偏好变化,
故对不认识 `mirror_urls` 的存量客户端完全透明。

#### Scenario: 缺省(未配偏好)时行为不变

- **GIVEN** 一个已发布 RN release,其 artifact 有一个已通过校验的 GitHub `mirror_url`,且 app
  未配置 platform 偏好
- **WHEN** 客户端检查更新且判定有更新
- **THEN** 响应 `has_update=true`、`download_url` 指向裸间接入口(302 解析到 OSS)
- **AND** `mirror_urls` 含该 artifact 的 `?source=github` 间接 URL

#### Scenario: 偏好 GitHub 时 OSS 降为 fallback 候选

- **GIVEN** 一个 app 对 `react-native-android` 配了 GitHub 优先,其 artifact 镜像已校验、且有
  S3 对象与活跃 backend
- **WHEN** 客户端检查到有更新
- **THEN** `download_url` 仍为裸间接入口(302 解析到 GitHub)
- **AND** `mirror_urls` 含 `?source=oss` 候选,且**不含** GitHub 候选

#### Scenario: 无其余可用源时字段为空

- **GIVEN** 一个 artifact 只有 S3、无 `mirror_url`(或镜像未通过校验)
- **WHEN** 客户端检查到有更新
- **THEN** 响应照常返回 `download_url`,且 `mirror_urls` 在响应中省略(空集不序列化)

#### Scenario: 存量客户端不受偏好翻转影响

- **GIVEN** 一个装了旧版 SDK(不认识 `mirror_urls`、无 failover)的客户端,其 app 刚被配成
  `react-native-android` GitHub 优先
- **WHEN** 它检查更新并直接下载 `download_url`
- **THEN** 该裸入口 302 到 GitHub,下载成功 —— 无需客户端发版
