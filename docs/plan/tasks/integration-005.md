---
id: integration-005
scope: server endpoint capability and versioned directory
status: done
depends-on: [integration-004]
---

## objective

服务端支持受信 provider 策略、正式 browser bridge 元数据和版本化完整目录；保留中央服务只处理协调元数据的边界。与004串行以避免 Store/Lua 冲突。此任务只提供协调能力，不宣称已有浏览器密码学传输实现。

## context

- `docs/browser-bridge.md`：字段/时限/修订、provider、cursor、原子采样的唯一合同。
- `docs/integration-contract.md`、`docs/redis-coordination.md`。

## path

`server/src`、`server/test`、`server/.env.example`、`server/README.md`。不改 SDK/Lab/根文档或线上。

## verification

真实 Memory/Redis 两 handler/JWK 验签走 Native 发布→SPA 新旧目录读取，同 owner/product/binding；错误角色/跨账号产品拒绝。provider 规范边界（port/path/编码/重定向/凭据）、策略收紧对原有 devices 接口同样有效。bridge 同地址版本只延长 expiry、pubRevision 拒绝迟到旧发布、撤回重发不能复活更旧版本、四项历史上限、24KiB。原子采样含 session+devices+Redis TIME；cursor scope/epoch/policy reset、不泄露全局变化，lease/入口过期影响内容 hash，单纯 lastSeen 不改变拓扑版本。全部服务测试/真实Redis0skip及build；独立审查后才合入。

## delivery

源码冻结 `70bfb807601f61ba6d65ef5a2d0354ab78a6e02d`，独立评审15项实际Redis/HTTP/ES384场景PASS；合入根分支 `851fa17`。合入后服务123/123（真实Redis、零跳过）、JS实际HTTP集成1/1及server build/typecheck通过。详见[评审](../reviews/integration-005.md)。本结论只覆盖协调元数据，不含SDK桥接适配、云登录、生产入口或手机公网验收。
