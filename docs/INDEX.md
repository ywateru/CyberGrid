# 文档

- [快速接入](getting-started.md)：登录、设备发现与端口映射。
- [系统架构](architecture.md)：身份、协调与端到端数据边界。
- [Redis 协调契约](redis-coordination.md)：共享状态、原子性、到期与多实例要求。
- [部署](deployment.md)：Logto、Docker / Node.js、Vercel Functions 与 Vercel KV 兼容配置。
- [SDK](sdk.md)：SDK 接口与恢复行为。
- [Lab 网页登录](lab-browser-login.md)：Mac / Android 官方 Logto SDK 接入。
- [发布](releasing.md)：构建、下载与发布产物。
- [当前交付](plan/README.md)：Redis / Functions 实现与验收任务。
- [AnyWork 集成反馈评估](plan/analysis/anywork-integration-2026-11-10.md)：历史基线的能力缺口、责任边界与建议；[当前覆盖表](plan/analysis/integration-progress.md) 维护逐项决定、优先级和交付证据。
- [产品集成契约](integration-contract.md)：产品允许策略、授权与状态、关闭、浏览器桥接的实施目标。
- [有限期注册准入契约](registration-admission.md)：RAM-only fence 的安全退役方案与必要反例；服务端已验证，SDK接入与生产迁移待完成。
- [浏览器桥接与目录扩展](browser-bridge.md)：受信入口、版本化完整目录和宿主安全适配边界（实施目标）。
- [Lab 远程访问开关](lab-remote-controls.md)：默认关闭、显式开启、保持本地服务与 ADB 控制（实施目标）。
- [RAM-only共同样例](common-sample.md)：官方授权owner、本地受限会话、浏览器桥接和实际长跑的模块/验收合同（实施目标）。
- [元数据与兼容性](metadata-and-compatibility.md)：逻辑到期/物理清理、平台/版本、授权窗口和 SDK 分发边界。
