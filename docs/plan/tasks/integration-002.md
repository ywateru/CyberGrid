---
id: integration-002
scope: js-authorization-state-contract
status: done
depends-on: []
---

## objective

扩展 JS SDK 的新鲜 token 请求、可订阅分层状态、固定错误分类和有界配置读取重试。保持只读目录权限、单一刷新者、未知交换结果处理、同身份与取消保护。

## context

- docs/integration-contract.md（第 2 节）
- sdk-js/README.md

## path

sdk-js/src、test、README.md，必要时仅新增 sdk-js examples。不得修改 server、Rust、根设计文档或线上环境。

## verification

状态订阅初始快照/单调版本/订阅者异常/退出迟到结果；minValidity 与 forceRefresh 一次限制；明确 401 与结果未知分开；身份切换仍拒绝；GET 重试预算与 AbortSignal；固定错误不输出秘密。保留现有 API 测试，构建和实际 service 集成回归通过（新服务产品协议在 integration-001 合入后补联调）。
