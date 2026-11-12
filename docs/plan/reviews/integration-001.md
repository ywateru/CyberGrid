# integration-001 独立验证

状态：**PASS**。最终冻结提交为 `2281198425a0620d223c22009ddf91e10e248e32`（父提交 `07fed67524f512130aba6263847fd8d910456dec`，相对原基线累计 12 个变更文件）。冻结工作树 clean；12 个文件的 SHA-256 均与最终开发 manifest 一致。

## 结论与已解决发现

静态产品策略可让同账号、同产品的多个真实 Native client 共享设备与信令命名空间，SPA 只读取其被允许产品；每个请求仍校验真实 client、当前策略和会话模式。静态模式无需 Management M2M。动态模式维持原一对一语义及旧有效回执恢复。

首轮冻结曾发现 P1：升级前未绑定 client/mode 的动态注册回执，使用同 owner、app、名称、registrationKey、revision、requestId 重试时错误返回 `409 REGISTRATION_ID_CONFLICT`。Native 与 Console 均可复现，原因是新增绑定改变 content/HMAC。最终修复仅在动态模式原一对一权限下，原子匹配旧回执、原 key hash、未绑定有效 session，以及 Native 原 fence 后返回原结果；不创建或迁移旧记录，所有新注册继续绑定真实 client/mode。该问题已在最终冻结提交复核关闭。

## 独立执行

使用真实 Redis 7.4.3、两个实际 loopback Node HTTP handler 和实际 `LogtoIdentity` ES384 JWT/JWK 验签，**12 个独立场景全部通过**：

- 同账号、同产品的两个 Native client 经不同 handler 发现设备及收发信令；跨产品/账号信令拒绝。
- 签名不正确、缺 connect scope、Native 请求 client 不匹配、集合外 client 和 Native/SPA 角色错误均拒绝。
- Console 只看到允许产品、canonical product ID、`source: static` 和显式端口匿名隧道地址；不能用 Native alias 或无权产品查询。
- 新 Native/SPA 同 client 回执在不同 handler 重放返回同 key；不同真实 client 复用 requestId 被拒绝。
- Native 被移组后旧会话拒绝；SPA 产品集合缩小后查询权限改变；撤销会话入口仍可用。
- 静态配置无 Management 凭据可启动；没有 Management directory 调用；明确错误配置拒绝。
- 旧动态 active session 仍可读取；static/dynamic 会话不能跨模式取得权限。
- 旧动态未知结果的 Native 和 Console 回执均恢复原 sessionKey。
- 恢复旧回执不改写 session metadata、不增加目录 revision；撤销、旧 session 超期或高 revision 接管后，旧请求不能复活权限。
- 修改内容、requestId、真实 Native client、owner 或切到 static，均不能借兼容路径取得原 key。不同 owner 可按自身权限注册独立会话。

复跑 JS 冻结提交 `f2da9a451bc7e2dfeaf8f90e20e7658812cc1500` 的 built ESM 与最终服务端组合，4 个独立实际 HTTP/Redis 集成场景全部通过，详见 [integration-002](integration-002.md)。

## 证据与验证边界

独立私有归档为工作区外 `integration-verify/independent-001-final-results.json`；首轮失败证据保留在 `independent-001-results.json`。测试使用本地生成的 ES384 签名密钥、token 和合成用户标识，未使用真实 Logto Cloud 登录。

开发方报告的首轮 95 项服务器套件、最终配置边界 10 项，以及窄修后受影响 Store/API/产品测试 40 项均通过，build/typecheck 通过；这些不是本次独立重跑套件数量。本报告不声称完成真实云租户授权、生产部署、Android 或 Wi-Fi/4G 业务通道测试。

静态 Native/SPA 角色由管理员声明，JWT 证明 client ID，不证明 Logto 应用类型。策略撤除仅在运行该配置的实例生效；旧实例/公开端点退役窗口仍需发布验收。旧未绑定 Console 回执没有历史 SPA client 字段，其兼容范围是固定 SPA 动态配置的既有信任边界，不能宣称可追溯历史 client 变更。RAM-only admission/fence 安全退役另见待实现设计，不计作本次交付。
