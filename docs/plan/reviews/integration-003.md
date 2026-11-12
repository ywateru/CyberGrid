# integration-003 独立验证

状态：**PASS**。最终冻结提交 `d4e56108d79a3653e8507294c127671106c5fb12`（父提交 `623a7e85609ac1cf0b07ca177a2c7cbcc247e068`，相对基线 `5e4b26c` 累计 10 个变更文件）。冻结工作树 clean，从 Git commit 导出私有源码副本并记录变更文件 SHA-256；未修改开发工作树。

## 结论与已关闭发现

Rust 远程入站 gate 默认关闭；显式 enable 后才发布保存的端口 intent。disable 先在本机关闭 gate、取消 peer/stream 和在途握手，再处理协调撤销；云端失败不能恢复本机访问。本机服务及可读取的端口 intent 保留。授权上下文、RAM-only provider、分层 watch 状态和发布确认边界满足本任务契约。

首轮独立验证发现两个 P1，已在最终冻结提交修复并复核关闭：

1. 未知交换固定重放旧配置，协调 epoch 变化后永久 `ExchangeOutcomeUnknown`。现在只在固定可信配置端点通过完整 OAuth binding 检查并明确返回新 epoch 后，才退役旧尝试并创建新幂等操作。同 epoch 的 409/401、配置读取失败或 binding 变化仍保留原 token/requestId/revision/name/key，不借此重新刷新或注册。
2. 首次 exchange 已确认但首 publication 回执丢失，session 已保存而 supervisor 尚未启动，导致永久 already-logged-in。现在已知 session 即由单个 supervisor 接管；显式重试、后台恢复及无 provider 的手工 token 重试均沿相同已验 subject、registrationKey 和更高 revision 恢复。背景恢复已 ready 时显式重试确认当前结果；排队的旧唤醒不能取消新 runtime。默认 gate 不随恢复开启。

## 独立执行

在精确导出的私有源码副本中追加独立验证代码，执行 **4 项新反例，全部通过**：

- 真实 TLS 1.3 双向证书认证、实际 smux 和本机 TCP 回显服务建立两个方向的业务流。默认关闭客户端不能接收反向流，显式 enable 后可用。协调撤销返回 503 时，disable 仍终止已有两个方向及新流，返回 localClosed=true、Pending；本机业务 TCP 继续可用、端口 intent 保留、旧 Client 不能重新开启。之后确认撤销才返回 Confirmed。
- 在已认证反向 mux 上只发送部分端口选择字节，关闭后再补齐。实际本机 listener 未接受连接，关闭前尚未完成的选择请求不能跨越 gate。
- 未知 exchange 后切换协调 epoch，同 Client 经可信配置识别新 epoch 后成功恢复，资源 token owner 仅在已确认的新域再次取 token。
- 首 publication 回执丢失后，同 Client 显式重试成功，未永久停在已登录但不可用状态。

另独立执行最终冻结副本的 **13 项针对性原有/新增测试，全部通过**，覆盖：

- provider 固定 issuer/client/resource/scopes、minValidity、代际、取消 token，以及明确 `401 INVALID_IDENTITY` 只刷新一次、相同请求 ID/revision；其他拒绝不刷新。
- `[599,409,401,200]` 未知重试 body 完全一致且 provider 仅一次；配置失败、issuer/client/resource/scope/token endpoint 变化不能替换原尝试；可信新 epoch 才新请求/token/revision，原 name/key 保持。
- 首发布失败后的显式、自动和手工 token 恢复；已知身份不变、注册 revision 递增、gate 仍关闭、重复 close 和排队旧唤醒安全。
- watch 初始状态与分层业务授权；旧 publication 响应不能确认较新的 intent；关闭后迟到响应不能恢复 publication ready。
- 关闭时等待已派发 rotation owner 完成，不丢弃其所有权；已验 subject 改变被拒绝；未确认的注册或已知 DELETE 失败仍返回 Pending；在途 TLS 握手被取消。

本机流测试没有模拟 TLS 或业务字节。协调 HTTP 服务、用户和资源 token 为测试 fixture，未把它们描述为真实 Logto Cloud 登录或真实 Redis 协调。RAM-only 路径没有创建 CredentialStore，也没有把 grant 交给 Rust 文件存储；宿主仍负责官方 SDK 的单一 rotation owner，旧 provider 默认适配不能自行保证强制刷新。

## 证据与验证边界

工作区外私有证据：`integration-verify/rust-003-final/source-final.json`、`rust-independent-final.log`、`rust-targeted-final.log`。首轮两个失败反例保留在 `integration-verify/rust-003/rust-independent.log`，没有覆盖失败历史。

开发方报告生产源码完整 60 项 Rust 测试通过，strict clippy all-targets 和 lab example 构建通过；最后仅测试/README 的追加又执行了相关两项及 clippy。这些不是本次独立全量套件重跑数量。本次独立结果为 4 项新反例加 13 项针对性测试。

Confirmed 只覆盖本 Client 在当前可信协调 epoch 中的已知 session 精确撤销，不证明历史旧部署的全局删除。可信新 epoch 允许退役旧未知域，旧公开部署的实际停用另按运维契约执行。关闭等待宿主已派发 owner 的完成；没有据此宣称任意宿主阻塞下固定一秒返回。

未操作生产服务、真实 Logto 账号、Android 或 Wi-Fi/4G 公网测试。Tauri 宿主显式启用适配、有限准入/fence 安全退役及后续目录能力属于其他任务，不计作本次已实现或已验证能力。
