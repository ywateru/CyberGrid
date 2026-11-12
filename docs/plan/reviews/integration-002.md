# integration-002 独立验证

状态：**PASS**。冻结提交 `f2da9a451bc7e2dfeaf8f90e20e7658812cc1500`，冻结工作树 clean。服务端 integration-001 的旧动态回执恢复问题已修复，并在最终组合上复核通过。

## 独立执行与结果

重新构建 SDK 的 ESM 和类型声明，执行 JS 测试 **31 通过，0 失败，0 跳过**。此外运行独立端到端 harness：使用该构建产物、integration-001 首轮冻结源码和最终冻结源码 `2281198425a0620d223c22009ddf91e10e248e32` 的实际 Node HTTP 服务、真实 Redis 7.4.3 和实际 `LogtoIdentity` 的 ES384 JWT/JWK 验签，以下 **4 个独立场景全部通过**。

1. 首个签名有效但无所需 scope 的 token 经实际服务返回 `401 INVALID_IDENTITY`；SDK 向宿主请求一次 `forceRefresh=true`，上下文为 `authorization-rejected`，`minValidity=60` 秒，保留原 requestId；新 token 可建立会话。通过静态产品目录查询 canonical product、设备和显式端口隧道 URL，跨产品查询被拒绝。
2. 实际 Redis 已提交 Console 会话后丢弃 HTTP 回执，再向 SDK 注入后续 `401 INVALID_IDENTITY`。SDK 保持 `exchange-outcome-unknown`，不触发刷新；第三次使用原 token 和 requestId 经实际服务恢复原 sessionKey。Redis 中只有一个新增 console session，注销后清理该会话。
3. 在 Redis 中撤销当前 console session，随后并发执行两个目录查询。SDK 对实际服务的失效响应只启动一次恢复，宿主上下文为 `coordination-recovery`；两次查询均恢复成功，身份保持一致。
4. 受信 SPA 配置下提交实际 Native token，实际服务返回 `APPLICATION_MISMATCH`；SDK 返回固定鉴权错误，不强制刷新绕过应用校验。

独立 harness 同时检查快照不可变、revision 单调、订阅者异常隔离、业务授权默认 unknown、publication/reachability 为 unknown，以及快照和实例的公开 JSON 不包含实际 fixture access token 或 sessionKey。unit tests 另外覆盖取消/迟到回调、配置 GET 有界退避、未知结果 8 次容量限制、不同身份恢复拒绝、固定诊断错误与畸形响应边界。静态审阅确认没有新增 Web Storage/文件凭据写入或 grant 刷新所有者。

## 产品与文档边界

`minValidity` 是向宿主提出的新鲜度要求；宿主必须实现同一个官方 Logto token owner 的刷新和缓存策略。SDK 不验证 token 的 exp 后据此认定身份，不接管 refresh token。`source: static` 表示管理员声明的产品策略，签名 token 的 client ID 仍由服务端验证。

当前 SDK 的未知债务容量保存在同一个实例内。README 中“不能通过重建 SDK 实例……绕过它们”是宿主必须遵守的集成约束，不应理解为当前 SDK 已提供跨实例或跨进程共享的全局预算；后者本次未实现。已向文档 owner 标明此范围。

本验证使用本地生成的 ES384 密钥和合成用户标识，未声称完成真实 Logto Cloud 登录、浏览器官方 SDK 刷新、生产部署、Android 或 Wi-Fi/4G 测试。上述 4 项独立集成场景已在服务端最终冻结提交上复跑且全部通过，integration-001 自身的 12 项独立场景另有复核报告。

私有验证证据为工作区外 `integration-verify/independent-002-final-results.json` 和 `js-unit-002.log`；无真实账号、token、密钥或生产数据进入源码报告。
