---
id: integration-007
scope: js directory watch and secure bridge extension
status: done
depends-on: [integration-002, integration-005]
---

## objective

JS 只读协调 SDK 增加原子完整目录解析/有界串行 watch、provider 验证及受信宿主安全 adapter 扩展；保持 console 无 Native/信令权限。桥接业务操作泛型透传但全部经 wrapper gate，不能交出可绕过关闭的底层 channel。AnyWork 实现只作外部宿主真实互通，不复制/打包其 AGPL 源码。

## context

- `docs/browser-bridge.md`：接口和安全/时限/取消合同。
- `docs/integration-contract.md` 第2/4/5节。
- `docs/metadata-and-compatibility.md`：平台与许可边界。

## path

`sdk-js/src`、tests、README/examples/package scripts。新 adapter 接口的确切 TS 类型在实现前报 root 对照合同确认，避免泛型裸 channel 逃逸。不改服务端/Rust/Lab/外部 AnyWork/生产。

## verification

真实服务+Redis+JWK+构建后 ESM：Native 发布/续期/撤回→JS directory watch→adapter→失效关闭，跨 scope/epoch/reset/目录不可用/乱序响应、取消/迟到返回均实际经过边界。单调期限不受 wall-clock 回拨影响，过期有独立计时器，握手完成前复核，业务 request/stream 同步 gate，close超时仍不能用。

外部真实 AnyWork Noise fixture 固定版本（只读），实际验证公钥/安装绑定、错误公钥、短授权 ACK 续期与拒绝、取消后迟到清理；不以 mock encrypted:true 代替 E2E。无实际宿主环境时记录缺口，不伪称互通通过。TS build/既有全部测试/源码和依赖许可扫描；后续手机/长跑仍属整体验收。

## delivery evidence

初次冻结`b5acbec`的两项P1已在修复冻结`a1d891c`闭环，[独立r2评审](../reviews/integration-007-r2.md)PASS。根分支顺序合入为`d081839`和`0465e75`，合入后单次完整JS **59/59**、实际旧HTTP **1/1**、真实Redis/双handler/ES384新bridge链 **1/1**均exit0、零跳过。根分支证据保存于私有`integration-007-root`目录；构建随上述npm脚本实际执行。

独立实际Noise **17 checks**通过，范围是精确Quick endpoint的本地映射与fixture业务授权，不是公网/真实Logto/手机/长跑。该模块done不代表010整体交付或下载/线上已更新，原BLOCKED报告与失败证据保留。
