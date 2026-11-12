---
id: integration-003
scope: rust-authorization-state-and-local-disable
status: done
depends-on: []
---

## objective

Rust 增加上下文 token provider、新鲜授权和分层 watch 状态、发布 intent/确认区分、显式开启/本机优先关闭远程。RAM-only 首次登录与恢复不要求 CredentialStore。保留现有三级传输、单飞恢复、真实 mTLS 和 grant 单所有者边界。

## context

- docs/integration-contract.md（第 2/3 节）
- docs/plan/analysis/anywork-integration-2026-11-10.md
- sdk-rust/README.md
- docs/lab-browser-login.md

## path

sdk-rust/src、examples、README.md、必要的 crate 元数据；不得修改 server、sdk-js、Tauri 宿主或线上。宿主显式开启适配单独联调。

## verification

真实流验证默认不开放、enable 后可达、disable 在线/离线均停止新流及已有双向流、本地业务继续可用；并发握手、恢复和迟到发布不能越过关闭 gate。状态 watch 初始值/代际/发布确认；provider 请求字段、明确拒绝刷新一次、结果未知不换 token/注册 ID、同身份保护及 RAM-only 无文件写入。构建/现有实质 Rust 测试及针对性反例通过；每项结果明确覆盖边界。
