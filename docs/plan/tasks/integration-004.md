---
id: integration-004
scope: server finite registration admission
status: done
depends-on: [integration-001]
---

## objective

实现可选 finite-v2 准入的服务端及 Memory/Redis 原子边界，使 RAM-only 注册键可在所有旧票失效后安全回收。默认 legacy-v1 的限制继续明确记录；不得将旧永久 fence 直接加 TTL。此任务不修改 Rust/JS 客户端，后续 SDK 通过公开能力协商接入。

## delivery

服务端提交 `f119362`、`f04544a` 已合入；独立评审最终9组场景PASS，根分支全服务107/107、真实Redis零跳过。见 [评审](../reviews/integration-004.md)。本结论不包含 Rust 协商接入或生产切换。

## context

- `docs/registration-admission.md`：首版协议、时限、编码、GC 和反例的唯一契约。
- `docs/integration-contract.md`：真实 OAuth client 与产品授权边界。
- `docs/redis-coordination.md`：原子性、跨进程、懒清理和容量。

## path

- `server/src`：config/service/types/store/redis-script 及新增内部 admission 模块。
- `server/test`：真实 Memory、Redis、HTTP/JWK 验签测试及必要夹具。
- `server/.env.example`、`server/README.md`。

不要修改根文档、SDK、Lab、生产环境或 GitHub。

## verification

- 类型检查、全服务测试，真实 Redis 必须运行且 0 skip。
- 真实两个 handler/Redis/JWK 验签：challenge→mint→exchange→publish→同产品发现；跨 owner/product/client/key/epoch 和改签名均拒绝。
- 同 key 跨域 deviceId 稳定；旧域更大 revision、旧 close、回执未知后重放、并发 mint CAS、同 requestId 改内容均覆盖。
- 可控 Redis 时钟须通过测试专用脚本变体或测试实例设施，不接受 runtime 客户端指定 now；至少跨过 24h/清理边界，证明旧 challenge/ticket 无法重建已回收 fence。
- 真实超过 128 次分窗口新 key 注册/关闭可继续，瞬时超容量仍拒绝；存在有效 session 时不早清理。
- legacy 与 finite 模式同 prefix 启动拒绝、缺票拒绝、旧永久 fence 不被新 GC 删除；原动态/静态授权和故障反例回归。
- 冻结提交、文件 hash 与独立评审后才合入；不得用仅 mock 的测试证明安全回收。
