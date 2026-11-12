---
id: integration-001
scope: product-policy-and-static-directory
status: done
depends-on: []
---

## objective

实现产品级 SPA/Native 显式绑定与静态允许配置；静态模式无需 Management M2M，保留严格 OAuth client 校验、产品/账号隔离和现有动态目录兼容。

## context

- docs/integration-contract.md（第 1 节）
- docs/plan/analysis/anywork-integration-2026-11-10.md
- docs/architecture.md

## path

server/src 的策略、配置、目录、服务和 session 存储类型/实现，相关 server/test，server/.env.example、server/README.md。不得修改 sdk-js、sdk-rust、根设计文档或线上环境。

## verification

真实 Redis/HTTP handler 验证两个 Native client 的同产品同账号发现与信令；跨产品/账号/未允许 SPA/Native 拒绝；静态模式没有 Management 网络调用；策略移除拒绝现有 session；两种模式与注册幂等/恢复语义保持。配置测试覆盖残缺、冲突、未知字段和版本、容量及固定错误脱敏。类型检查及相关真实服务测试通过，输出确切覆盖边界。
