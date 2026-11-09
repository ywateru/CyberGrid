---
id: redis-002
scope: deployment
status: in-progress
depends-on: [redis-001]
---

## objective

构建实际 Vercel Node Function，配置独立 Redis namespace，验证预览后切换正式域名，生产路径不再依赖 xgy-w。提供一键部署与配置文档。只使用已授权免费资源，不启用自动付费升级。

## context

- docs/deployment.md
- docs/redis-coordination.md

## path

vercel.json、server/release、server/public、scripts/release、.github/workflows、README.md、server/README.md、docs。

## verification

构建输出包含 Function 与必要静态文件，不包含私密资料或大型 SDK 制品。Vercel 预览与正式请求确实进入新 Function / Redis，而非 rewrite 到旧服务。凭据仅存服务环境；Logto 配置与回跳保持可信。
