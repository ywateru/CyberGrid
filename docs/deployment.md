# 部署 CyberGrid Node 服务

CyberGrid 由一个长期运行的 Node.js 22 进程提供完整官网、管理后台、Logto 控制台和 `/api/v1/`。协调状态只在该进程的有界内存中，不需要数据库。不要配置多副本、Node cluster 或 serverless Function；外部 HTTPS 入口可以使用普通 Nginx、Caddy 或隧道，服务不依赖特定平台。

## 配置 Logto

在自己的 Logto 租户中准备：

- 一个 SPA Application，用于网站 `/console/`。登记完整的回跳 URI，默认 `https://你的域名/console/callback/`，以及该 HTTPS origin。
- 启用 Device Flow 的 Native Applications。每个 SDK 使用自己的真实 Application ID，两端分别授权。
- 一个固定 API resource，提供 `connect` scope；签发面向该 resource 的 ES384 用户 access token。优先使用显式 `LOGTO_RESOURCE`；未设置时，有公开 origin 则默认该 origin 加 `/api`，否则默认固定标识 `https://cybergrid.local/api`。该标识是 OAuth audience，不需要 DNS 或实际 HTTP 服务，但必须在 Logto 创建完全相同的 resource。
- 读取应用目录的 M2M Management API 凭据。当前使用的 `all` 权限较宽，须只保存在服务端；运行代码只读取固定租户目录，不写入用户或应用。

SMTP、用户注册和邮件验证在 Logto 中配置。不要把 M2M、SMTP 或管理后台密钥放进 SDK、前端或 Git。

## Docker：一个命令启动

安装 Docker Engine / Docker Desktop 与 Compose 后：

```sh
cd server
cp .env.example .env
chmod 600 .env
# 编辑 .env，填写 Logto 配置；公开 origin 可留空
docker compose up --build -d
```

容器以非 root 用户运行 Node，包含 POSIX Python3 租约 helper。构建上下文只有 `server/` 的明确源码和站点文件，不包含真实 `.env`、SDK、provider 或测试制品。Compose 在运行时注入 `.env`，默认仅发布 `127.0.0.1:7400`，用宿主机 HTTPS 反向代理连接它。容器内的 HTTP 端口用于入口转发；`CYBERGRID_PUBLIC_ORIGIN` 可选，显式设置时必须是最终 HTTPS origin。不设置时，HTTPS 代理必须覆盖单值 `X-Forwarded-Host` / `X-Forwarded-Proto`，Node 用它们识别本次访问地址。

Compose 的命名卷只保存固定租约 inode，不保存会话、设备或信令。保持一个服务实例，不要 `--scale`；不要删除运行中租约文件。停止或更新使用 `docker compose down` / `docker compose up --build -d`。更新会清空协调内存，SDK 按既有恢复协议重新注册。

## 直接运行 Node

需要 Node.js22+、POSIX Python3 标准库 `fcntl`：

```sh
cd server
npm ci --include=dev
cp .env.example .env
chmod 600 .env
# 编辑配置
npm run build
npm start
```

`npm start` 自动读取存在的 `.env`。`npm run dev`、`npm run owner` 和旧的 `src/owner.ts` / `src/local.ts` 都进入同一生命周期，不创建第二份协调服务。生产安装可在构建后 `npm prune --omit=dev`；运行依赖包含 `tsx`。

| 配置 | 用途 |
| --- | --- |
| `CYBERGRID_PUBLIC_ORIGIN` | 可选，留空按每次请求地址派生；配置时固定为对外 HTTPS origin，不含路径、凭据、query 或 fragment |
| `LOGTO_ISSUER` | 固定租户 HTTPS issuer，以 `/oidc` 结尾；同时用于精确 CSP |
| `LOGTO_BROWSER_CLIENT_ID` | 独立 SPA client_id |
| `LOGTO_MANAGEMENT_CLIENT_ID` / `LOGTO_MANAGEMENT_CLIENT_SECRET` | 目录读取的 M2M 凭据，仅服务端保存 |
| `CYBERGRID_ADMIN_KEY` | 可选，独立随机 32–256 UTF8 字节；为空时管理 API 返回503 |
| `LOGTO_RESOURCE` | 固定 OAuth audience；未显式指定时，有公开 origin 则默认其加 `/api`，否则为 `https://cybergrid.local/api` |
| `CYBERGRID_CONSOLE_REDIRECT_URI` | 可选，同 origin 专用回跳；默认 `/console/callback/` |
| `CYBERGRID_BIND` / `PORT` | Node 默认 `127.0.0.1:7400`；Compose 在容器内设为 `0.0.0.0:7400` |
| `CYBERGRID_OWNER_LOCK` | 固定租约路径；默认当前目录 `.cybergrid-owner.lock` |
| `CYBERGRID_MEMORY_LIMITS` | 可选 JSON 容量限制，必须符合 MemoryStore 的边界 |

回跳不能使用 `/`、`/api`、`/admin` 或 `/healthz` 保留路径；在 Logto SPA 注册精确 URI。Node 直接返回控制台页面，保留 URL 中的 code/state query，不依赖平台 rewrite。`CYBERGRID_DEV_MODE=1` 只允许 loopback 请求 origin，显式公开 origin 也须 loopback，且生产环境拒绝此开关；不提供假登录。

## HTTPS、限流与存活检查

配置公开 origin 时，API URL、同源判断与默认回跳固定使用它，不让请求头覆盖。留空时，每个请求使用合法 Host 和 socket HTTP/TLS，或严格单值 `X-Forwarded-Host` / `X-Forwarded-Proto`（仅 `http` / `https`）；重复、多值、非法 authority、userinfo、control、非法端口均拒绝，Forwarded 不受支持。代理必须覆盖这两个头，不能追加或透传客户端自带值。派生地址仅用于本请求 URL、同源校验和回跳，不触发服务器向该地址 fetch，不缓存或首请求绑定。

JWT issuer、JWKS 和 audience 始终独立固定。已有部署删除 `CYBERGRID_PUBLIC_ORIGIN` 前，将原来的 API resource 显式写进 `LOGTO_RESOURCE`，以免回退到新默认标识；不必修改 Logto 租户。默认回跳为本次有效 origin 加 `/console/callback/`，配置接口始终返回绝对 URI；显式回跳必须与本次有效 origin 一致。不同 Host 请求不会污染其他请求或改变 JWT audience。

客户端 IP 只取 socket 地址，忽略 X-Forwarded-For。若所有请求经过一个外部代理，它们会共享该代理的 IP 限流桶，不能靠注入转发头获得独立配额。

`GET /healthz` 和 `HEAD /healthz` 只报告进程存活，不调用 Logto、不泄露配置或协调状态。请求从读 body 到 handler / 响应读取共用25秒预算，body 上限100000字节、响应上限2MiB，信令长轮询最多15秒；客户端断连释放等待者。SIGTERM 停止接入、关闭协调等待者与连接并释放租约；SIGKILL 后 helper 通过 stdin EOF 自动释放内核锁。

`/admin/` 使用同一服务 `.env` 中的独立管理密钥，只读查看协调状态；密钥只保存在当前页面内存，刷新后重新输入。`/console/` 使用 Logto，提供当前用户自己的只读设备视图。

## SDK 与发布

[GitHub Releases](https://github.com/ywateru/CyberGrid/releases) 提供 Go、Rust、Lab 和校验清单。网站构建不下载这些大文件。原生制品通过独立的 Release workflow 构建；Node 服务 CI 只检查新服务与 Docker，不运行手机或 WAN 验收。正式签名、公证、商店发布与长期监控需另行配置。
