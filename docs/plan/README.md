# Redis 与 Functions 交付

本轮将单进程 RAM 协调迁移至 Redis，同时支持 Node / Docker 和 Vercel Functions。用户已授权实现、提交与部署，并明确允许 Functions；旧服务器仅在新服务验收后退出生产路径。

实现、部署与独立验收分工见任务文件。设计来源为 [Redis 协调契约](../redis-coordination.md) 和 [部署](../deployment.md)。

| 任务 | 范围 | 状态 |
| --- | --- | --- |
| [redis-001](tasks/redis-001.md) | 异步 Store、原子 Redis、Node / Functions、Docker | done |
| [redis-002](tasks/redis-002.md) | Vercel 构建、免费 Redis、预览与正式域名 | in-progress |
| [redis-003](tasks/redis-003.md) | 独立多实例、重启、竞争与线上路径验收 | in-progress |

本轮不改变端到端传输协议，不重复构建已发布原生客户端。Wi-Fi / 4G 的实机传输验收独立记录，不能用 Redis / CI 通过代替。

源码基线 `9488018` 已通过独立复核与 21 项真实 Redis 反例检查；开发方全服务 80 项、最终定向 13 项、JS HTTP 集成以及托管 Redis 的 REST / TLS TCP 检查均通过。部署与 Docker 制品验收仍单独推进，详见 [评审](reviews/redis-003.md)。
