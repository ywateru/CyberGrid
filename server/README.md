# CyberGrid Node.js 服务

一个长期运行的 Node.js22 进程同时提供完整网站、Logto 控制台、管理后台与 `/api/v1/`，API 直接调用同进程唯一有界 MemoryStore。协调状态不写磁盘、不使用数据库；中央不接收或转发端到端业务数据。

完整 Logto 参数、HTTPS 入口、容量和恢复边界见 [部署文档](../docs/deployment.md) 与 [架构](../docs/architecture.md)。

```sh
cp .env.example .env
chmod 600 .env
# 编辑 Logto SPA/M2M 配置，可选公开 origin 与独立 admin key
docker compose up --build -d
```

Docker 以非 root 用户运行，构建上下文只包含本目录明确允许的文件。真实 `.env` 在运行时注入，SDK/provider/私有数据不进入 image。默认对宿主 loopback 发布7400，由通用 HTTPS 反向代理提供对外域名。

直接运行需要 Node.js22+ 与 POSIX Python3：

```sh
npm ci --include=dev
npm run build
npm start
```

`start`、`dev`、`owner` 使用同一入口与租约。生产构建后可 `npm prune --omit=dev`；`tsx` 是运行依赖。`CYBERGRID_PUBLIC_ORIGIN` 可留空：配置时固定使用该 HTTPS origin；未配置时从本请求合法 Host / socket HTTP/TLS 派生，也支持严格单值 `X-Forwarded-Host` 与 `X-Forwarded-Proto`。外部代理必须覆盖这两个头，不能追加或透传客户端输入；Forwarded 不受支持。API IP 限流始终只采用 socket 地址，外部代理后的用户共享代理 IP 的限流桶。

`LOGTO_RESOURCE` 是启动时固定的 OAuth audience，不随 Host 改变。未显式配置时，有公开 origin 则兼容默认该 origin 加 `/api`，否则使用固定标识 `https://cybergrid.local/api`（无需 DNS，但要在 Logto 创建相同 resource）。已有部署移除公开 origin 前须显式保存原 resource。

`GET/HEAD /healthz` 是不调用 Logto 的进程存活检查。信令长轮询最多15秒；全请求预算25秒，body 上限100000字节、响应上限2MiB。不要运行多副本或 Node cluster。固定 POSIX flock 租约只保证单进程，不保存协调状态；重启产生新 epoch，SDK 重新注册与发布端口。

默认 SPA 回跳为本次有效 origin 加 `/console/callback/`，可配置同源专用路径，并在 Logto 登记相同 URI。根路径和 `/api`、`/admin`、`/healthz` 保留路径不可用。Node 自行提供回跳页面，保留 query。静态服务只访问 public tree，拒绝路径穿越、隐藏文件和符号链接，CSP 精确放行配置的 Logto issuer origin。

管理后台使用独立 `CYBERGRID_ADMIN_KEY`，只读展示白名单 DTO；不配置时 API 返回503。前端不内置密钥。Logto 管理用户和应用，中央不管理应用目录，也不向 Logto 写协调数据。

开发检查：`npm run build` 与 `npm run test:node` 覆盖本轮入口、静态/回跳、请求边界和租约；`npm test` 保留服务协议测试。GitHub Actions 的 Node workflow 构建并启动真实 Docker image，原生 Release workflow 独立运行。

设备只接受关闭 Device Flow 的普通 Native 应用，认证配置声明 `loginFlow: "authorization_code"`；SPA 个人控制台保持独立。Lab 使用官方 SDK 网页登录，新旧认证策略需配套升级，不能迁用其他 client 的授权。
