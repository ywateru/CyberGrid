# 部署 CyberGrid

CyberGrid 支持 Node.js 22 / Docker 和 Vercel Functions。两种入口使用同一套网站、API、Logto 校验与 Redis 协调逻辑。Redis 保存共享会话、设备、端口和信令；不需要 PostgreSQL，不依赖另一台 CyberGrid 服务器。实例重启不清空协调状态。

## 配置 Logto

在自己的 Logto 租户中准备：

- 一个独立 SPA Application，用于网站 `/console/`。登记完整回跳 URI，默认 `https://你的域名/console/callback/`，以及该 HTTPS origin。
- 关闭 Device Flow 的普通 Native Application。Mac 和 Android 使用同一个真实 `client_id`，各自独立授权。登记 Mac 回调 `http://127.0.0.1:17896/callback` 与 Android 回调 `tech.tinadev.cybergrid.lab://tech.tinadev.cybergrid.lab/callback`。已有 Device Flow 应用需要替换，不能迁用另一个 client 的 refresh token。
- 一个固定 API resource，提供 `connect` 用户 scope，并向测试用户分配相应角色。显式设置 `LOGTO_RESOURCE` 最清楚，例如 `https://cybergrid.local/api`；它是 OAuth audience，不需要解析或实际 HTTP 服务。Logto 须签发面向该 resource 的 ES384 用户 access token。
- 读取应用目录的 M2M Management API 凭据。当前使用内置 Management API 的 `all` 权限，须只保存在服务端；运行代码只读取目录。

SMTP、用户注册、邮件验证与发件域在 Logto 配置。不要把 M2M、SMTP、Redis 凭据或管理后台密钥放进 SDK、前端或 Git。

## Docker

安装 Docker Engine / Docker Desktop 与 Compose：

```sh
cd server
cp .env.example .env
chmod 600 .env
# 填写 Logto 配置，使用 Compose 内部 Redis
# REDIS_URL=redis://redis:6379
docker compose up --build -d
curl --fail http://127.0.0.1:7400/healthz
```

Compose 启动 Node 和带健康检查、AOF 与持久化卷的 Redis。容器以非 root 用户运行 Node，默认发布宿主 `127.0.0.1:7400`，通过宿主 HTTPS 反向代理提供外部地址。构建上下文仅包含服务源码与站点文件，不包含真实 `.env`、SDK/provider、客户端授权或测试制品。

普通更新使用 `docker compose up --build -d`；停止使用 `docker compose down`，默认保留 Redis 卷。`docker compose down -v` 会删除协调数据，不用于常规更新。Redis AOF 和卷降低重启丢失风险，不能替代备份与宿主磁盘可靠性保证。

Node 实例可以连接相同 Redis 和 namespace，对外负载均衡由宿主配置。Compose 默认的固定端口面向单容器便捷部署，扩容时须自行调整端口与入口。各实例容量配置必须一致。仓库 Compose 显式选择内部 Redis；使用外部 Redis 时，应采用下述直接运行方式，或修改自己的 Compose，移除内部 Redis 连接覆盖与依赖后注入外部配置。

## 直接运行 Node

需要 Node.js 22+ 和可用 Redis：

```sh
cd server
npm ci --include=dev
cp .env.example .env
chmod 600 .env
# 填写 Logto 与 Redis 配置
npm run build
npm start
```

`npm start` 自动读取存在的 `.env`。`dev`、`owner` 与 `start` 都进入同一 Node 生命周期；不再使用 Python / POSIX flock。生产安装可在构建后 `npm prune --omit=dev`，`tsx` 是运行依赖。

## Vercel Functions

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2Fywateru%2FCyberGrid&env=LOGTO_ISSUER,LOGTO_RESOURCE,LOGTO_BROWSER_CLIENT_ID,LOGTO_MANAGEMENT_CLIENT_ID,LOGTO_MANAGEMENT_CLIENT_SECRET,CYBERGRID_REDIS_TRANSPORT,KV_REST_API_URL,KV_REST_API_TOKEN&envDescription=Set%20CYBERGRID_REDIS_TRANSPORT%20to%20vercel-kv%20and%20connect%20Marketplace%20Upstash&envLink=https%3A%2F%2Fgithub.com%2Fywateru%2FCyberGrid%2Fblob%2Fmain%2Fdocs%2Fdeployment.md%23vercel-functions)

本项目在 Vercel 上推荐使用 **Vercel KV 兼容配置**，由 Vercel Marketplace 的 **Upstash Redis** 提供托管存储。根据 [Vercel 官方说明](https://vercel.com/docs/redis)，旧版独立 Vercel KV 已停止提供，原实例于 2024 年 12 月自动迁移到 Upstash Redis。新项目在 Marketplace 创建或连接资源即可，无需自建 Redis；底层仍使用 Redis 协议与 Lua 原子操作，不是另一个独立 KV 引擎，也不需要安装已弃用的 `@vercel/kv`。

1. 在 Vercel 导入仓库，项目根目录使用仓库根目录，采用仓库的 `vercel.json`，不要将 Root Directory 改成 `server/`。
2. 在项目 Storage / Marketplace 选择 [Upstash Redis](https://vercel.com/marketplace/upstash/upstash-kv)，创建或连接资源，选择要连接的 Vercel 环境。集成会注入 `KV_REST_API_URL`、`KV_REST_API_TOKEN` 等变量；直接使用它们，不必复制成 `UPSTASH_REDIS_REST_*`。需要完整写权限 token，不能使用 `KV_REST_API_READ_ONLY_TOKEN`。选择适合流量的方案，免费方案不要启用自动付费升级。
3. 设置 `CYBERGRID_REDIS_TRANSPORT=vercel-kv`，并填写下表的 Logto 参数。部署按钮中的此项也填 `vercel-kv`；按钮不会自动创建存储资源。使用按钮首次创建项目时，可填已有资源的 KV 变量，或先导入项目、连接存储，再重新部署。每个环境使用独立 `CYBERGRID_REDIS_PREFIX`，预览不能误用生产 namespace。
4. 部署后将实际 HTTPS origin 和完整 `/console/callback/` 回跳登记到 Logto SPA。首次部署可以暂不设置 `CYBERGRID_PUBLIC_ORIGIN`；设置后必须与实际访问域名一致。
5. 绑定自己的域名并再次核对 Logto 回跳。打开官网、`/console/` 和 `/admin/`；`/healthz` 检查实例与 Redis 可用，还要通过配置/认证 API 验证 Logto。

Vercel 服务端的存储变量如下，URL 与 token 由集成注入，不要提交真实值：

```dotenv
CYBERGRID_REDIS_TRANSPORT=vercel-kv
# KV_REST_API_URL 和 KV_REST_API_TOKEN 由 Marketplace 集成注入
CYBERGRID_REDIS_PREFIX=cybergrid:production
```

预览环境使用另一个 prefix，例如 `cybergrid:preview`。Marketplace 可能同时注入 TCP 和 REST 地址；显式 `vercel-kv` 只采用 KV REST 这一组，因此不会误选其他连接。`KV_URL` 和只读 token 不参与此模式。

已有部署切换连接方式时，继续连接原资源并保留原 prefix 和容量配置，不要照抄示例 prefix 覆盖生产值。切换前后核对共享 epoch 一致，无需清空数据。Vercel 环境变量更新后重新部署才会生效；生产候选应先检查成功再切换正式域名。项目无需 `CYBERGRID_OWNER_URL`、`CYBERGRID_PROXY_SECRET` 或旧服务 rewrite 地址。平台的存储接入步骤见 [Marketplace Storage 官方文档](https://vercel.com/docs/marketplace-storage)。

网站和 API 由同一个 Node Function 入口处理，包含安全头、静态路径校验和控制台回跳。Function 冷启动复用 Redis 里的会话，不通过隧道或代理转发到旧 Node 主机。Function 请求时限为 30 秒，内部 HTTP 预算 25 秒，信令等待不超过 15 秒。

Vercel 的环境变量修改需要重新部署后生效。免费 Redis 有存储、命令和带宽配额，持续长轮询与心跳可能耗尽命令额度；免费不表示可以无限持续在线。额度耗尽或 Redis 不可用时 API 拒绝协调请求，不退回本机内存。

## 环境变量

| 配置 | 用途 |
| --- | --- |
| `CYBERGRID_REDIS_TRANSPORT` | 可选，精确为 `vercel-kv`、`upstash-rest` 或 `tcp`；Vercel 推荐 `vercel-kv`，只采用所选组。留空不等于未设置；自动模式须完全不配置此变量 |
| `KV_REST_API_URL` / `KV_REST_API_TOKEN` | `vercel-kv` 使用的 Vercel Marketplace 变量，HTTPS REST 地址与完整权限 token，必须成对存在 |
| `REDIS_URL` | `tcp` 使用的标准 Redis TCP/TLS 连接，例如 Compose 的 `redis://redis:6379` |
| `UPSTASH_REDIS_REST_URL` / `UPSTASH_REDIS_REST_TOKEN` | `upstash-rest` 使用的直接 REST 配置，必须成对存在；不能使用只读 token |
| `CYBERGRID_REDIS_PREFIX` | 协调 namespace，默认 `cybergrid:v1`；生产、预览与测试分别设置，不含花括号 |
| `CYBERGRID_LIMITS` | 可选 JSON 共享容量限制，只能下调以下硬上限；同 namespace 所有实例必须一致 |
| `LOGTO_ISSUER` | 必填，可信租户 HTTPS issuer，以 `/oidc` 结尾；JWT 和 CSP 均固定使用它 |
| `LOGTO_BROWSER_CLIENT_ID` | 必填，独立控制台 SPA 的 client ID |
| `LOGTO_MANAGEMENT_CLIENT_ID` / `LOGTO_MANAGEMENT_CLIENT_SECRET` | 必填，目录读取 M2M 凭据，仅服务端保存 |
| `LOGTO_RESOURCE` | 固定 OAuth audience；建议显式设置。默认有公开 origin 则为该 origin 加 `/api`，否则 `https://cybergrid.local/api` |
| `CYBERGRID_PUBLIC_ORIGIN` | 可选，对外 HTTPS origin，不含路径、凭据、query 或 fragment；留空按每次请求地址识别 |
| `CYBERGRID_ADMIN_KEY` | 可选，独立随机 32–256 UTF-8 字节；未设置时管理 API 返回 503 |
| `CYBERGRID_CONSOLE_REDIRECT_URI` | 可选，同 origin 专用回跳；默认 `/console/callback/` |
| `CYBERGRID_BROWSER_ORIGINS` | 可选，逗号分隔精确 HTTPS origin，开发允许 HTTP loopback；仅开放只读 JS SDK 的 console 接口，默认只允许同源 |
| `CYBERGRID_BIND` / `PORT` | 普通 Node 默认 `127.0.0.1:7400`；Compose 容器内为 `0.0.0.0:7400`；Functions 无需配置 |

回跳不能使用 `/`、`/api`、`/admin` 或 `/healthz` 保留路径；在 Logto SPA 注册精确 URI。已有部署删除公开 origin 前，显式保存原来的 `LOGTO_RESOURCE`，避免改变验签 audience。请求 Host 不选择 Logto 租户、JWKS 或 OAuth resource。

未配置 `CYBERGRID_REDIS_TRANSPORT` 时，只允许一组完整连接配置；残缺 REST 配置或多组同时存在会拒绝启动。显式选择后只读取所选组，未知值、空值或所选组缺失时拒绝启动，不尝试其他存储组。Docker Compose 固定使用内部 TCP Redis；`.env` 中不要为该默认 Compose 设置 `vercel-kv`。使用外部托管存储请按上文直接 Node / 自定义 Compose 方式部署。

共享存储默认容量也是本实现的硬上限。Lua 操作有锁内扫描，不能通过环境变量任意放大到数百万记录；更大规模需要调整索引与执行策略。容量配置在 namespace 创建时固定，不能在实例之间分别调整。

| 类别 | 全局上限 | 单用户 / 单设备上限 |
| --- | --- | --- |
| 会话 | 512 | 每用户 128 |
| 设备 | 256 | 每用户 64 |
| 信令 | 1024 条 / 2 MiB | 每用户 512 条 / 2 MiB；每设备 128 条 / 1 MiB |
| 注册 fence | 2048 | 每用户 128，namespace 内永久保留 |
| 幂等记录 | 4096 | 随相应有效期清理 |
| 限流桶 | 2048 | 各接口按自身速率校验 |
| 长轮询等待者 | 128 | 每设备 2 |

## HTTPS、限流与存活检查

配置公开 origin 时，API URL、同源判断与默认回跳固定使用它；留空时采用合法 Host 和 socket HTTP/TLS，或严格单值 `X-Forwarded-Host` / `X-Forwarded-Proto`。代理必须覆盖这两个头，不能追加或透传客户端值。重复、多值、非法 authority、userinfo、非法端口与非 HTTP(S) scheme 均拒绝。派生 origin 仅用于本次请求，不缓存，不触发服务端向该地址 fetch。

普通 Node 的客户端 IP 限流采用 socket 地址，忽略客户端可伪造的 X-Forwarded-For；外部代理后的请求可能共享 IP 限流桶。Functions 对登录与管理员 IP 桶使用固定 `vercel-shared` 标识，不信任转发 IP，因此不同访问者会共享这些入口配额。会话级配额仍独立校验；所有配额存入 Redis，不能靠切换实例获得新配额。

`GET /healthz` 和 `HEAD /healthz` 检查实例与 Redis 共享状态是否可用；Redis 不可用时返回 503。不调用 Logto，不暴露配置或协调状态。body 上限 100000 字节、响应上限 2 MiB；长轮询有 Redis 等待租约和取消机制，不依赖后台定时清理。Node 停机关闭本实例的接入与连接，保留共享数据。

Redis epoch、注册密钥派生材料与 fence 属于 namespace 的身份状态。不要手工删除部分键来解除配额；删除整个 namespace 代表明确的协调重置。完整到期、幂等与原子性约束见 [Redis 协调契约](redis-coordination.md)。托管 Redis 的复制、网络分区与故障切换保证由提供商决定，本项目不提供跨故障域的共识协议。

Upstash 的当前保证与配额以官方 [EVAL](https://upstash.com/docs/redis/commands/scripting/eval)、[REST API](https://upstash.com/docs/redis/features/restapi)、[Global Database](https://upstash.com/docs/redis/features/globaldatabase)、[一致性](https://upstash.com/docs/redis/features/consistency) 和 [价格](https://upstash.com/pricing/redis) 为准。写路由 EVAL 内的原子校验不能提升为跨分区、跨故障切换的线性一致承诺。

`/admin/` 使用独立管理密钥，只返回白名单投影；管理密钥仅在当前页面内存保存。`/console/` 使用 Logto，显示当前用户自己的只读设备视图。独立前端除 Logto SPA 回跳外，还须加入 `CYBERGRID_BROWSER_ORIGINS`；该白名单不为业务端口代理鉴权或 CORS。

## SDK 与发布

[GitHub Releases](https://github.com/ywateru/CyberGrid/releases) 提供 Rust、Go、JS、Lab 和校验清单。网站构建不下载 SDK 大文件。服务 CI 验证 Node、Functions 适配与 Redis / Docker；原生 Release workflow 独立运行，不把 CI 构建当作手机或公网直连验收。
