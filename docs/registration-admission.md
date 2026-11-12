# 有限期注册准入契约

本契约补齐 [产品集成契约](integration-contract.md) 的 RAM-only 注册键生命周期。服务端 finite-v2 已实现并经独立验证；Rust SDK 接入与生产迁移尚待完成，状态见 [覆盖表](plan/analysis/integration-progress.md)。默认 legacy-v1 的永久 fence 仍按原契约工作，不允许直接给旧 fence 增加 TTL。

## 不变量

- logical deviceId 仍由已验 owner、product、registrationKey 派生；准入续签不能改变同一 Client 的远端定位 ID。安装 ID 只做展示关联。
- 准入域绑定 epoch、owner、product、真实 authClientId、registrationKey hash。签发序列只由 Redis 原子递增，不采用客户端时间或随机数大小判断先后。
- 同一 logicalID 仅最新域能注册，域内继续执行 registrationRevision/requestId/content fence。旧域即使有更高 revision 也不能接管新域。
- 注册请求不能在回执未知时自动申请新域；复用原 challenge、mint requestId、ticket 和注册 requestId 直至已确定结果或已明确过期。
- 所有提交时效均以 Redis TIME 再检查。Node 只做签名和身份绑定验证，不能用进程时钟替代提交校验。

## 创建链

1. 宿主为 SDK 提供已授权资源 token。服务端验证真实身份和产品允许策略。
2. challenge 读取 Redis 时间、epoch 和该 logicalID 的 latestDomain，返回短期 HMAC 绑定的 challenge（包含 expectedLatestDomain、nonce、owner/product/client/keyhash/notAfter）。challenge 不要求永久保存。
3. mint 携带原 challenge 和独立 requestId。Node 验签后，Lua 先查同内容幂等回执，再检查 challenge 时效及 expectedLatestDomain CAS；成功分配全局唯一 mintOrder，原子更新该 logical fence 的最新域及已签票据的最晚到期上界。
4. 同 requestId/内容已有有效 mint 回执时，重放返回原 ticket，允许原 challenge 已过期；ticket 到期后仍明确拒绝。没有有效回执时，过期 challenge 不能再创建域，不能用新的 OAuth token 给旧 challenge 重新签票；新授权需要新的显式 challenge/mint。
5. Node 用已有协调 seed 的分域 HMAC 派生签名密钥，构造确定的 ticket。register 携带 ticket，服务端验签/验身份，Lua 再检查域、期限、epoch、keyhash、revision 和容量，注册提交后才接管旧 active session。

HMAC 使用标准 SHA256 及固定域标签，密钥不离开服务端；challenge/ticket 按凭据处理，不写日志。不得采用自定义加密协议。下文冻结首版时间窗、编码和有限容量；实现仍须独立反例验证。

## 首版线协议与启用方式

`CYBERGRID_REGISTRATION_MODE=finite-v2` 启用有限准入；未配置时维持 `legacy-v1`，保留原容量限制。两种模式不共用一个 Redis prefix；切换时必须选择新 prefix，并退役原公开部署，不能认为新 handler 能约束旧 Lua。服务启动持久记录模式，不允许不同模式的实例接入同一个 prefix。MemoryStore 同样记录模式。

Native auth config 在有限模式增加 `registrationAdmission: {version:2,challengeSeconds:120,ticketSeconds:86400}`；legacy 不返回此字段。console 不需要注册准入。有限模式 Native exchange 缺少 `admissionTicket` 时返回 `409 REGISTRATION_ADMISSION_REQUIRED`，不会降级。协调协议的 publish/signal 与 logical deviceId 派生保持原样。

新增两个只接受 Native 资源 access token 的端点，使用与 exchange 相同的真实 client/product 校验和限流，不接受 console session：

1. `POST /api/v1/auth/registration/challenge`，body 为 `{appId,registrationKey}`；返回 `{challenge,epoch,expiresAt}`。`appId` 是真实 Native client，registrationKey 按既有规范验证。
2. `POST /api/v1/auth/registration/admit`，body 为 `{appId,registrationKey,challenge,requestId}`；返回 `{admissionTicket,epoch,expiresAt}`。requestId 是 UUID。

两个 body 均另带 `accessToken`，与当前 exchange 一致，不新增 query token 或记录 token。请求必须通过既有 JSON 大小、CORS、身份验签和产品策略边界；只为被允许的 Native 签发。两个端点不能靠未验证 JWT 字段构造 owner。

challenge 和 ticket 都是不透明的 `base64url(JSON数组).base64url(HMAC-SHA256)`。JSON 数组字段顺序固定，不接受扩展字段；base64url 必须规范且不得带 padding，解码后重编码字节必须一致；长度各不超过 4096 字节。签名比较使用 timingSafeEqual。分域分别为 `cybergrid-registration-challenge-v2` 和 `cybergrid-registration-ticket-v2`，密钥来自当前 epoch 的既有协调 seed。payload：

- challenge：`[2,epoch,userId,productId,authClientId,keyHash,expectedDomain,nonce,issuedAt,notAfter]`；nonce 为服务器 UUID；expectedDomain 为上次域的十进制字符串，无历史为 `"0"`。
- ticket：`[2,epoch,userId,productId,authClientId,keyHash,domain,issuedAt,notAfter]`；domain 为 Redis 原子分配的正安全整数转为规范十进制字符串，不能由客户端指定。

userId/产品/client/keyHash 来自已验身份与规范派生，不从 payload 直接信任；各字段精确匹配后才调用原子提交。Redis TIME 返回 challenge 的 issuedAt，notAfter 为 issuedAt+120000；mint 使用 Redis TIME 生成 ticket issuedAt，notAfter 为 issuedAt+86400000。存储操作在提交时检查 `now < notAfter`。进程时钟不决定提交有效性；不通过客户端字段缩短/延长窗口。

mint 幂等键绑定 epoch、owner、真实 client、requestId；内容包含原 challenge 原文与 keyHash/product。同 requestId 不同内容返回冲突；同内容且原 ticket 尚未到期时返回原 ticket 所需的确定字段，即使 challenge 的 120 秒已过。回执未知时重放原操作，不以新 challenge/requestId 绕过。原 ticket 过期后即使原回执还在也不签发可用新票，返回 `REGISTRATION_ADMISSION_EXPIRED`；不存在有效回执时必须先检查 challenge 未过期才允许 mint。回执的物理到期为该 ticket 的 notAfter，与已有注册/信令幂等记录共用总容量上限；没有额外无限表。

mint 使用 expectedDomain CAS，成功后以单调域替换 latestDomain 并更新 maxIssuedNotAfter；mint 本身不注销旧 session。register 的内容摘要和 sessionKey HMAC 必须包含 ticket/domain，防止同 requestId 换票混用。register 仅接受 latestDomain，域内执行既有 revision fence；session 与设备记录的 expiresAt 不超过 ticket.notAfter。后续 publish 不延长该绝对期限。

finite 模式的 register 回执与 mint 回执均保留到其 ticket/session 绝对期限，接管不能提前删除旧 register 回执再接受同 requestId 的新 ticket 内容。旧 session 撤销/接管后重放仍明确失效或冲突，不创建新会话。它们共同占用既有幂等容量；高频同 key 接管也可能先触幂等配额，不承诺窗口内无限重启。

固定错误增加：`REGISTRATION_ADMISSION_REQUIRED`、`REGISTRATION_ADMISSION_EXPIRED`、`REGISTRATION_ADMISSION_CONFLICT`、`REGISTRATION_ADMISSION_INVALID`；错误文本不回显票据/签名/身份。过期是该操作不可再提交的确定结果，但不证明以前没有成功；SDK仍需检查/清理原 session，不能把未知旧结果当作从未发生。

SDK 的续签必须区分已确认 session 和未知提交。已确认 session 在 ticket 自然到期后的正常协调恢复，可通过唯一 AuthProvider 请求同 subject/issuer/client/resource/product 的新鲜授权，以有界的 challenge/mint 建立新域；上下文为 coordination-recovery，要求 forceRefresh，并保持本 Client 的 registrationKey 和逻辑 deviceId。没有可用 owner 或授权失败时明确需要登录，不无限续签，也不因恢复自动开启本机 remote gate。

未知 mint/register 即使后来收到明确过期，也不能走上述后台续签路径；停止旧操作重放并保留原操作和关闭待确认事实。只有宿主显式重试登录并获得新鲜同身份授权，或按产品集成契约从完整可信配置确认协调 epoch 已改变，才可开始新域。新操作不能抹去旧的清理事实、换号隔离或迟到响应归属；过期和未知关闭结果分别记录。

上述同epoch显式重试以服务明确返回准入过期为前提。长时间断网后原access token可能先失效，精确重放仅得到401；401不是准入过期证明，本地wall clock也不能提供Redis提交结果。此时保留unknown及原操作，不能自动换token/票据/请求ID规避；当前版本没有另一个权威attempt-status接口，须明确报告这个恢复限制。该限制不应被宣传为任意断网时长均可自动恢复。

同Client显式fresh续域还要求此前已经取得可信Session.user，可验证新结果仍为原subject。首次mint/register结果一直未知、从未确认owner时，即使准入已明确过期也保持Unknown/Pending；不能解析不透明challenge/ticket或未验签JWT来补出可信身份。宿主应先关闭该Client，重新核对官方owner后显式创建新Client并重新授权/开启，旧清理事实独立保留，不静默复制并公开身份未确定的旧业务意图。

## 生命周期与物理回收

mint 新域本身不终止旧 active session；register 成功才替换 incarnation/session。旧域的迟到 close 只可撤销其原 session，不得退役更新域。

fence 必须记录 maxIssuedNotAfter，并在所有曾签发的旧 ticket 都已过期、相关有效 session 已结束后才能物理回收。无状态 challenge 不延长该时限：在已有 fence 时签发的 challenge 带非零且全局不复用的 expectedDomain，fence 回收后与无历史的 `"0"` 不相等，CAS 必须拒绝；在无历史时签发的 challenge 最多 120 秒有效，早于其后成功 mint 的 24 小时 ticket 到期。故回收不要求所有无状态 challenge 都已过期，但须证明它们不能重新命中已清理域。mint 回执保留至相应 ticket 不可能再被接受；不能先回收幂等回执再无条件接受旧 mint。本版 session 有效期不超过 ticket 准入到期，不隐含延长权限。

窗口内仍执行既有每用户/全局注册键、有效设备、幂等及签发速率上限，mint 在首次创建 fence 前检查容量。目标是避免随进程历史永久累积，不承诺任意频率无限重启。满额时返回 `CAPACITY_EXCEEDED` 和有界 retryAfter；它不是保证该时间后容量已空。

只有有限模式产生的 fence 可回收，记录 `admissionVersion=2`、latestDomain、maxIssuedNotAfter；不得回收 legacy fence。清理前检查所有关联 session 均已结束，且 `now >= maxIssuedNotAfter`；session/设备先清理，再处理 fence。全局域计数器不得随单个 fence 的回收而归零，达到安全整数边界时明确拒绝，不回绕。操作还需保持 Redis wrong-type/capacity preflight，避免 Lua 部分写入后报错。

## 版本与降级

新注册方式需明确能力协商及产品策略，不能让同一产品的旧 v1 register 绕过过期票据限制。要求有限准入的产品对缺失票据的注册返回需升级，不自动退回无限期注册。

Client一旦在可信配置中见过finite-v2，同一协调epoch的后续配置若移除registrationAdmission，应明确协议/配置不可用并保留原pending/config，不能自动降级legacy。未知版本或null同样不作为未广告处理；服务端Store的模式检查不替代SDK这项协商约束。

旧永久 fence 缺少票据时效证明，不能用新规则盲删。保留旧模式时必须继续说明旧容量限制；迁移与旧部署停用单独验收。旧二进制服务可执行旧 Lua，因此仅增加新 metadata 字段不能宣称已经防住仍在线的旧服务。

## 必须验证的反例

- 同 registrationKey 续签保持 deviceId；旧域大 revision、旧 close、旧 epoch、错误 owner/product/client、错误 keyhash 均不能越权。
- mint/注册回执丢失后重试返回原结果；旧 mint 回执清理后，过期 challenge 不重新产生域。
- 并发 mint 的 expectedLatestDomain CAS；取消或超时后迟到 mint 不悄悄替换已经建立的新域。
- 到期前 fence 不被回收；存在有效 session 时不提前回收；到期后旧票无法重建，新的明确授权可继续注册。
- 多轮 RAM-only 新进程超过原累计 128 次后，在窗口容量与速率条件满足时仍可使用；并发/瞬时超额依然拒绝。
- 现有 map listener 的远端 ID 不变；旧业务流仍按原恢复契约断开，不承诺无缝续传。
