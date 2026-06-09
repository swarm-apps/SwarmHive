# SwarmHive 生产部署示例

一份「复制即用」的单机 docker compose 样例:**swarmhive-server**(GHCR 镜像,内嵌 admin SPA)
+ **Postgres 17** + **RustFS**(S3 兼容存储)。

> 它与仓库根 `docker-compose.yml`(只跑 dev bundled-storage)和 CLAUDE.md 里手动 `docker run`
> 的 `swarmhive-pg` / `swarmhive-rustfs` / `swarmhive-mailpit` **互不接管**:独立 project 名
> `swarmhive-deploy` + 独立卷,避免 `down --remove-orphans` 误删开发数据。

> **用 Coolify?** 见 [`coolify/docker-compose.yml`](coolify/docker-compose.yml) —— 它用 Coolify 的
> magic 变量自动出域名 / TLS / 随机密钥,几乎一键。完整步骤见文档
> [用 Coolify 一键部署](https://github.com/swarm-apps/swarmhive/blob/main/apps/docs/content/docs/self-host/coolify.mdx)。

## 快速开始

```bash
cp deploy/.env.example deploy/.env
# 编辑 deploy/.env:改强密码、跑 `openssl rand -base64 32` 填 SWARMHIVE_SECRET_KEY、
# 设 SWARMHIVE_SERVER__BASE_URL 为对外 URL、(推荐)设 SWARMHIVE_BOOTSTRAP_OWNER_EMAIL。

docker compose -f deploy/docker-compose.yml up -d
```

镜像默认拉 `ghcr.io/swarm-apps/swarmhive-server:latest`。要钉版本就把 compose 里的 `:latest`
换成 `:0.1.0` 之类的语义化 tag。

## 首启

1. 访问 `http://<host>:3030/` —— 用户表为空时 admin SPA 会把你引到 `/setup`,填邮箱 / 显示名 /
   密码建第一个 **Owner**(密码 ≥12 位、≥3 种字符类别)。若设了 `SWARMHIVE_BOOTSTRAP_OWNER_EMAIL`,
   只接受这个邮箱认领。
2. **接入存储**(SwarmHive 存储是 **S3-only**,RustFS 只是自带选项之一):
   - **用自带 RustFS**:admin「设置 → 存储」向导选 *bundled RustFS*,endpoint 填
     `http://rustfs:9000`、bucket `swarmhive`(`rustfs-init` 已预建)、access/secret 填 `.env` 里的
     RustFS 密钥 → 测试连通 → 激活。CLI 等价:`swarmhive storage init rustfs --endpoint
     http://rustfs:9000 --bucket swarmhive --access-key-id … --access-key-secret …`。
   - **用外部 S3 / R2 / MinIO / OSS**:删掉 compose 里的 `rustfs` + `rustfs-init` 两个 service,
     向导选 *Existing S3 / Aliyun OSS*,填你的 endpoint / bucket / 凭证即可(server 不依赖 RustFS)。
3. **配邮件**(可选):admin「设置 → 邮件」加一个真实 SMTP provider 并激活,邀请 / 重置密码邮件才会发出。

## 生产 TLS

样例直接暴露 `3030`,**不内置反代**。生产请在 server 前套**你自己的**反向代理(nginx / caddy /
traefik / 云 LB)做 HTTPS,把对外 `:443` 域名转发到 `server:3030`,并把 `SWARMHIVE_SERVER__BASE_URL`
设成 `https://你的域名`。想强制只走反代,把 server 的 ports 改成 `127.0.0.1:3030:3030`。

## 升级

```bash
docker compose -f deploy/docker-compose.yml pull server
docker compose -f deploy/docker-compose.yml up -d server
```

镜像内 `config/default.toml` 的 `auto_sync = true` 会在启动时跑 sea-orm schema-sync,新版本的
表结构变更自动落库。要更可控的迁移再单独评估。

## 镜像与二进制从哪来

- **镜像**:`server/v*` tag 触发 `.github/workflows/server-release.yml`,推 `linux/amd64` +
  `linux/arm64` 多架构镜像到 GHCR。
- **单文件二进制**:同一工作流同时把 `x86_64` / `aarch64-unknown-linux-gnu` 的
  `swarmhive-server-<ver>-<triple>.tar.gz`(内含二进制 + `config/default.toml`)挂到 GitHub Release。
  不想用容器时下载解压,`cwd` 备好 `config/default.toml` 后直接 `./swarmhive-server` 即可。
