---
id: integration-006
scope: rust admission and endpoint capabilities
status: done
depends-on: [integration-003, integration-004, integration-005]
---

## objective

Rust SDK 接入 finite-v2 challenge/mint/register 能力协商，保留未知操作精确重放；公开 browser bridge 发布/续期/撤回意图和安装展示关联，按固定 endpointPolicy 校验正式 provider。使用新完整目录，保持本机 gate、实际 peer 认证和三传输路径。

## context

- `docs/registration-admission.md`。
- `docs/browser-bridge.md`。
- `docs/integration-contract.md`。
- 对应已合入服务端真实字段/端点及 SDK README。

## path

`sdk-rust` 的 source/tests/examples/README，必要 crate 配置。消费兼容仅允许 `apps/tauri-test/src-tauri/src/browser.rs` 的AuthConfig测试字段初始化及两处绑定比较：复用清楚的OAuth/固定产品不可变绑定接口，忽略已重新验证的能力metadata更新，不放宽真正OAuth或产品变化。不得修改其他Lab路径、服务端、JS或生产。

## verification

真实 Node handler/Redis/JWK fixture 与 Rust：config→challenge→mint→register→publish→目录读取，跨两个 Native 同产品设备定位稳定；未知 mint/register/响应取消/到期/epoch reset/换号不绕过幂等。legacy 不广告能力时保留原协议，required 能力未知明确拒绝。正式 provider device authority→固定/connect 保留显式端口且真实 WS+mTLS，port HTTP 与 bridge WSS 不混用。默认gate/disable、已有mTLS/iroh/WebRTC回归。RAM-only 不写凭据文件或私钥；fresh token owner唯一。locked build/clippy/tests及独立评审。

## delivery evidence

初次冻结`8ae0999`的公开explicit-fresh入口阻断已在修复冻结`d952d25`闭环，[独立r2评审](../reviews/integration-006-r2.md)PASS。根分支顺序合入为`26af081`、`cd3e891`；合入后完整Rust SDK **78/78**（`--include-ignored`，零跳过）、最新Lab宿主含ADB **28/28**、消费者`--locked`构建均exit0。旧冻结树的lifecycle断言在当前组合中已通过，未修改或放宽测试。根验证日志/命令/hash保存在私有证据标签`integration-006-root`。

独立公开原反例及五项新边界均实际通过；首次未知owner、未知401和旧清理Pending的保守语义未放宽。fixture时间/本地JWK与真实Redis验证不等同24小时自然长跑或Logto云登录。源码模块done不代表生产切换finite-v2、共同样例、下载包或手机/WAN已完成。
