# CyberGrid 协调服务

Node.js 22 / Docker 与 Vercel Functions 共用网站、Logto 控制台、只读管理后台和 `/api/v1/`。Redis 保存跨实例共享的短会话、设备、端口和信令；中央不接收或转发端到端业务内容。

完整配置见 [部署说明](../docs/deployment.md)，数据与原子性边界见 [架构](../docs/architecture.md) 和 [Redis 协调契约](../docs/redis-coordination.md)。Vercel 从仓库根目录导入，采用根 `vercel.json`；不要把项目根目录改为本目录。

```sh
cp .env.example .env
chmod 600 .env
# 填写 Logto 配置，Compose 使用 REDIS_URL=redis://redis:6379
docker compose up --build -d
```

Compose 包含带 AOF、健康检查和持久化卷的 Redis，Node 容器以非 root 用户运行。默认向宿主 loopback 发布 7400，由 HTTPS 入口提供外部访问。真实 `.env` 仅在运行时注入，不进入镜像。

直接运行需要 Node.js 22+ 和 Redis，无需 Python：

```sh
npm ci --include=dev
npm run build
npm start
```

`start`、`dev`、`owner` 都使用同一 Node 入口。生产构建后可 `npm prune --omit=dev`；`tsx` 是运行依赖。普通 Node 可使用 `REDIS_URL` 或直接 Upstash REST。Vercel 使用 `CYBERGRID_REDIS_TRANSPORT=vercel-kv`，直接读取 Marketplace 注入的 `KV_REST_API_URL` / `KV_REST_API_TOKEN`。旧 Vercel KV 已迁移为 Upstash Redis，该选择器提供环境变量兼容接入，不需要 `@vercel/kv`。配置分组、部署按钮和迁移步骤见 [Vercel 配置](../docs/deployment.md#vercel-functions)。

未设置选择器时只允许一组完整连接参数；显式选择后只采用所选组。所选参数缺失、选择器无效或存储不可用时拒绝请求，不改用其他存储，也不退回 RAM。`CYBERGRID_REDIS_PREFIX` 隔离环境，同 namespace 使用一致的 `CYBERGRID_LIMITS`。默认 Compose 使用内部 TCP Redis，不应设置 `vercel-kv`。

`CYBERGRID_PUBLIC_ORIGIN` 可留空；设置时固定使用该 HTTPS origin，留空时从本次合法访问地址派生回跳。可信代理必须覆盖单值 X-Forwarded-Host / Proto。`LOGTO_RESOURCE` 是固定 audience，不随请求 Host 改变。已有部署移除公开 origin 前须显式保存原 resource。

默认 SPA 回跳为 `/console/callback/`，须在 Logto 登记完整 URI。设备仅接受关闭 Device Flow 的普通 Native 应用，Lab 使用官方 SDK 网页登录。中央不管理应用，也不向 Logto 写入协调数据。

`GET/HEAD /healthz` 检查实例与 Redis，不调用 Logto；Redis 不可用时返回 503。长轮询最多 15 秒、全请求预算 25 秒，body 上限 100000 字节、响应上限 2 MiB。多个实例通过 Redis 原子校验协调；停止单个实例不会清空 Redis，也不更换 epoch。`memory-v1` 保留为客户端线协议标识。

管理 API 使用独立 `CYBERGRID_ADMIN_KEY`，未设置时返回 503。它只返回白名单 DTO，不展示会话密钥、信令正文或设备地址材料。前端不内置该密钥。

检查包括类型与站点构建、服务安全边界、Functions 适配，以及真实 Redis 的多实例注册、发现、信令、配额、过期与重启恢复。MemoryStore 仅作为测试夹具，不由生产配置创建。GitHub Actions 的服务检查与原生 SDK / Lab Release workflow 独立运行。
