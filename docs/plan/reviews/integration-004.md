# integration-004 独立验证

状态：**PASS**。最终冻结提交 `d971797aae539610fc85ac4f38b7e96f7068a274`，parent `0da3b51b4c98c2ecd39a586c61f7c32944d9a835`，任务 base `2bf8a5a1590ebc62477092347c7741598bdbde11`；独立核对最终工作树 clean，开发冻结清单的 10 个文件 SHA256 全部相符。本报告记录源码审查和隔离本地验证，不代表能力已合入或部署。

## 已关闭问题

**I004-P2-01，已关闭：MemoryStore 撤销设备后，新增的 deviceExpiries 条目无法回收。** finite register 将 logical deviceId 写入 deviceExpiries；revoke 删除当前 peers 记录，却未删除其 expiry；cleanup 只遍历仍存在的 peers，因此已撤销设备的历史 expiry 条目永远不会被访问。独立使用 registrationKeys=1、连续三个超过 24 小时的窗口，每次创建新 key、mint/register/revoke/cleanup，均观察到 devices=0、fences=0，而 deviceExpiries 数量为 1、2、3。这会绕过有限 fence 的容量回收，令 RAM 状态继续随历史设备数增长。

最终修复仅在真正删除当前 peer 的原有 incarnation guard 内同步删除 expiry。独立重跑原三窗口反例，六类索引均回收为零；另测三个窗口内同 key 换域接管，正常旧 close 与主动保留旧 session 的隔离反例均不删除新 incarnation 的期限，当前 close 才清理。后一个反例直接覆盖 guard 的不匹配分支，并非只因 takeover 已删除旧 session 而通过。独立审阅者未修改实现。

## 独立验证

独立私有 harness 使用实际 Redis 7.4.3、随机 loopback 端口、隔离 AOF/noeviction 实例及自动清理。时间边界通过仅测试使用的 RedisTransport 替换唯一 Redis TIME 语句，实际 Lua 提交仍在真实 Redis 执行；未给生产 API 加入 caller-supplied now。最终冻结上七组真实 Redis/HTTP 独立场景与两组 Memory 修复反例合计 **9 通过、0 失败、0 跳过**：

1. challenge 精确 120 秒边界、原 mint 回执至 ticket 到期前仍返回同票、到期后拒绝；仍有效的非零 expectedDomain challenge 在 fence GC 后 CAS 拒绝，后续域不复用。
2. Node 已验 challenge 后，实际 mint 提交跨过截止时间被拒绝；新增 expiry 索引的 wrong-type 在业务写入前拒绝，域计数、fence、回执、注册及会话保持不变。
3. 最后一个安全整数域可以签发一次，下一次拒绝；即使 fence 已回收，全局域计数也不回绕。
4. 共享幂等容量已满时 register 拒绝且不部分写入；mint 新域不注销当前 active session。
5. 同 requestId 更换 ticket/content 被拒绝；成功 register 才接管，旧 close 不删除新 incarnation；心跳不能延长 ticket 的绝对到期，截止时 publish 被拒绝。
6. 两种模式混用同 prefix 被拒绝；legacy 永久 fence 在跨窗口清理后仍保留。
7. 两个实际 Node HTTP handler 共用 Redis，经真实 LogtoIdentity ES384 JWT/JWK 验签完成 challenge → mint → exchange → publish → 同产品发现；secondary Native client 映射 canonical product；跨 owner/client/key 被拒绝；跨 handler 幂等重放及 challenge 到期后原票恢复均通过。

8. 原始 Memory 三个 24 小时窗口反例在最终修复上通过；撤销后 expiry 立即清空，窗口到期后 peers/fences/expiry/sessions/mint/register 回执全部为零。
9. 三窗口同 key 换域后，正常旧 close 和刻意保留旧 session 的反例均保留新期限及 peer；只有当前 incarnation 撤销才清理 expiry。

静态审查覆盖固定 HMAC 分域、规范 base64url、固定数组字段和身份绑定、Lua TIME 提交检查、域 CAS、mint 独立回执表与 register 回执保留、绝对 TTL、模式持久化、wrong-type/capacity 预检。MemoryStore 回收问题已在最终提交闭环，本轮无剩余实施阻断。

开发者报告的首轮全服务 106/106，以及修复后的相关 34/34（真实 Redis、0 skip）和 build/typecheck PASS，与独立最终九组验证分别记录；独立审阅者未重复运行全服务套件。最终相对首轮只变更 memory.ts 与 admission.test.ts；Redis Lua、签名、身份、服务端配置均保持原冻结字节。私有证据为工作区外 `integration-004-independent/counterexamples-final.log`、`counterexamples.mjs` 及 `review-final.json`；首轮反例与结论仍保留于对应 first 文件，报告未收录凭据、票据、JWT、私钥或生产数据。所有动作仅限隔离本地 fixture，未触碰生产、Logto Cloud、应用授权或设备。
