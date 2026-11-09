---
id: redis-001
scope: server
status: done
depends-on: []
---

## objective

生产使用 Redis 权威协调状态，异步 Store 与 Lua 提交时鉴权覆盖 Node / Docker 和 Functions。移除单进程锁依赖；配置错误和存储故障拒绝请求，不退回 RAM。

## context

- docs/redis-coordination.md
- docs/architecture.md
- docs/deployment.md

## path

server/src、server/api、server/test、server/package*.json、server/.env.example、server/Dockerfile、server/compose.yaml。

## verification

真实 Redis 的两个独立 handler / store 验证注册、发现、信令、撤销、并发 revision、TTL、配额、取消和重启恢复。HTTP、Logto、CORS、静态页面和管理员投影继续符合原有安全边界。
