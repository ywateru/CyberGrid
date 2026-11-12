# 产品集成契约（实施目标）

本文件定义 AnyWork 反馈改进的实施目标。各能力是否已交付以任务及评审为准；现网版本不能据此视为已经具备全部能力。分析来源见 [反馈评估](plan/analysis/anywork-integration-2026-11-10.md)。

## 1. 产品与身份

应用仍在 Logto 创建和管理。CyberGrid 增加服务端允许策略，不增加应用 CRUD 或应用数据库。固定 issuer/resource 与客户端类型验证继续保留。

新增可选 `CYBERGRID_PRODUCTS` JSON：

```json
{"version":1,"products":[{"id":"nativeDesktop","name":"Example workspace","nativeClientIds":["nativeDesktop","nativeMobile"],"browserClientIds":["workspaceWeb"]}]}
```

- 产品 id 使用一个规范 Native client ID，长度/字符范围与现有应用 ID 一致，必须包含于该产品 nativeClientIds；产品 id 是协调命名空间，实际 OAuth client ID 单独记录。当前不任意扩展 appId 线协议长度。
- 产品、客户端集合必须非空、无重复，Native client 不能属于多个产品；同一 SPA 可以显式允许多个产品，不能同时作为 Native。客户端 ID 限 `^[A-Za-z0-9_-]{1,21}$`。未知版本、未知字段、格式错误和容量超限在启动时拒绝。JSON 最多 128 KiB，产品数最多 32，每产品每类客户端最多 16，名称最多 256 字符。
- 配置存在时采用静态策略，M2M 不必配置、不触发 Management 网络调用；配置错误不退回动态模式。完全未配置时保留现有 LogtoDirectory 模式，继续严格检查 Native 类型、Device Flow 和固定 SPA。
- 公开配置顶层返回 policyVersion（对产品/集合排序规范化配置的 SHA256，64 位小写十六进制）及 products（仅含当前客户端允许的 `{id,name}`）；Native 另含 productId，原 clientId 是实际客户端。SPA 配置支持显式 clientId 查询，缺省使用 LOGTO_BROWSER_CLIENT_ID；任意未允许的客户端不能取得可用绑定。
- 交换仍要求请求 appId 精确等于已验 token 的 Native client ID。Store 以 productId 隔离设备/信令，并保存真实 authClientId。响应 appId 保持实际 Native client ID，额外返回 productId；不使旧 SDK 的 appId 校验失效。
- Console 会话保存已验证 SPA client ID，应用目录/设备查询仅返回其被允许的产品。同账号同产品的不同 Native client 可发现和连接；跨账号、跨产品、集合外客户端拒绝。每次请求按当前进程策略检查真实 client 和规范 product 映射，客户端移组不能继续使用旧 session；注册回执 content/HMAC 也绑定真实 client 和模式，避免跨客户端错用 requestId。
- 新会话另存 policyMode（static/dynamic）。已有动态模式会话缺少 authClientId 的兼容范围仅限原始固定 SPA/Native 一对一语义；static 拒绝无模式或模式不同的旧 session，不得据此获得产品分组权限。切换模式需要重新交换授权，不清空其他 namespace。
- 静态配置中 Native/SPA 角色由部署管理员显式声明，JWT 证明真实 client_id，不证明 Logto 应用的 type/DeviceFlow；管理员需在 Logto 核对角色，不能声称无 Management 调用也实时验证了这些元数据。动态模式保留原有类型查询。
- 允许策略在进程启动时固定；移除项在运行新策略的实例生效，不代表仍持旧配置的 worker/旧公开部署被即时全局撤销。发布需切换并停用旧实例/旧端点，明确最坏窗口；不能用单 handler 测试证明跨所有部署即时撤销。需要共享即时策略 fence 时单独设计，不能由 worker 相互覆盖策略版本。
- 动态目录模式新增 LOGTO_MANAGEMENT_ORIGIN（HTTPS origin）和 LOGTO_MANAGEMENT_RESOURCE（固定 HTTPS resource），默认分别为 issuer origin 与该 Management origin 加 `/api`；token endpoint 仍是 issuer/token。固定配置不得由请求 Host 或客户端输入决定。静态模式并不等于 Logto 账号撤销即时生效。

SDK 重新读取可信配置时，OAuth 不可变绑定与可更新能力元数据分别校验。固定 server/issuer/真实 client/resource/scopes/OAuth endpoints 和已选择 product 不随响应迁移；product policyVersion、允许产品列表、endpointPolicy、coordination 和 registrationAdmission 只能在其各自合同下重新验证后更新。当前 client 被移除、移组或原 product 不再被允许时明确失败并封闭远程访问，不能自动选另一个产品。OAuth grant 所有权/交接的比较只使用完整不可变授权绑定，不能把能力更新等同于刷新 grant 换主，也不能借此忽略真正的授权绑定变化。

## 2. 宿主授权、状态与错误

宿主为刷新 grant 的唯一所有者；SDK 只向该所有者请求资源 access token。Rust 保留旧 AuthProvider 实现的源码兼容入口，并增加可承载请求上下文的方式；JS 扩展已有回调参数。

请求上下文包含固定 issuer/clientId/resource/scopes、minValidity（秒，默认 60，允许 1..3600）、requestReason（connect/coordination-recovery/authorization-rejected）、forceRefresh 和取消/代际信息。首次请求 forceRefresh=false。交换明确返回 401 INVALID_IDENTITY 且此前无未知结果时，只允许一次 forceRefresh=true，仍绑定同一身份与 requestId；应用不匹配/其他拒绝不通过刷新绕过。未知交换结果保留私有原 token 和原 requestId，显式重试重放原交换，不再请求宿主刷新或创建新注册尝试。该未知尝试后即使收到 401，也不当作从未提交，不换 token/id 绕过未确认会话数量上限。不能直接解析未验签 JWT 后信任身份。宿主明确无法刷新时返回需登录，不无限刷新。正常协调 session 的更新与业务授权更新分层处理。

未知交换的 epoch 例外：显式重试可重新读取固定可信服务的 auth config，并完整验证既有 issuer/client/resource/scope/endpoints。只有该可信配置明确提供不同 coordinator epoch，才允许结束对旧 epoch 的重放并用新 epoch 创建新的幂等操作；任意交换 401/409、配置读取失败或不匹配不能提供这个依据。同 epoch 必须保持原 token/requestId/revision。此例外证明当前协调域已改变，不证明其他尚未退役的旧公开部署没有旧会话；旧部署退役与原会话到期仍按运维/保留合同处理。

SDK 快照至少分开 identity、coordination、publication、reachability；业务授权状态由宿主提供，默认 unknown，不假定已授权。快照带单调 revision、SDK generation、epoch、固定 reason 和可重试性；不含 token、私钥或带凭据的 URL。订阅首先同步当前快照，后续按 revision 更新；订阅者异常不能破坏 SDK 状态机。Rust 的 broadcast 丢事件后可重新获取快照。

Rust 增加 AuthTokenRequest 和支持该上下文的 AuthProvider 方法，旧 provider 方法通过默认适配保留源码兼容；默认适配不能声称保证了强制刷新，由宿主实现新方法满足新契约。新增 provider 驱动的首次登录入口，手工 access-token 登录仍可用。当前 subject 已建立后，任何更新必须同身份；401 刷新规则与 JS 一致。快照采用 watch 接口供读取初始值及后续值，保留原有事件订阅。

发布返回/快照区分本地 intent 已接受与服务端 revision 已确认。保留离线 intent；未获确认不能展示远端已就绪。连接就绪不等于业务已授权。

首次 exchange 已确认但首个 publication 未确认时，session 仍归本 Client 所有，不回退成从未登录，也不永久停在不可重试的 already-logged-in。恢复 supervisor 在已知 session 后即获得生命周期所有权；显式重试或后台恢复沿相同已验 subject、registrationKey 和更高 revision 恢复，后台已完成时显式重试确认当前结果。默认 remote gate 和本地 intent 不因恢复而自动开启。失败状态与已保存会话分别记录，取消/关闭继续清理精确的已知或迟到会话。

JS 固定错误码保留，增加固定 stage/reason/retryable/retryAfterMs/本地诊断 ID，不携带原始 provider 错误。只对幂等配置 GET 做有界退避，鉴权/配置不匹配立即失败；注册/发布使用现有 fence/幂等标识处理不确定结果。用户连接有总预算；后台恢复可以独立持续，但阶段、下一次重试和停止条件可观察。取消阻止迟到成功重新激活。

## 3. 本机关闭、RAM-only 与身份

远程访问默认关闭，由宿主明确启用。停止发布与关闭远程分别建模：关闭远程先在本机禁止新流并终止已有远程流，再撤回发布和撤销协调会话。云端失败不反向开启本机。操作返回 localClosed 与 coordinatorRevocation 的真实结果；不代替退出 Logto，不停止本地工作区。

Rust SDK 增加显式 enable_remote / disable_remote。本次新 SDK 默认不允许入站业务流；expose 仍可保存本地 intent，只有明确开启后才公布端口。关闭时先原子封闭本机 gate，取消既有及正在建立的远程 peer/stream，再尽力撤销协调 session（成功时同时撤回设备发布）；保留本地业务服务和可供宿主读取的端口 intent。关闭后任何心跳/恢复/迟到握手不得再次发布或放行。disable_remote 终止当前 Client 的单次登录生命周期；重新开启由宿主明确创建新 Client，沿同一内存 token owner 完成资源授权/恢复 intent 并 enable，不复活旧 Client 或旧 TCP，也不要求无故退出 Logto。原生测试宿主和样例应增加明确开启动作，不能以编译兼容为由悄悄启用。

disable_remote 的返回结构必须保留本机关门成功的事实，即使云端撤回/撤销失败；本机关门不是“目录删除已确认”。仍处于 OAuth grant 交接或旋转中的私有 owner 继续完成 checkpoint/撤销，不能因 UI 取消丢弃其 future。

RAM-only 参考集成不创建 CredentialStore，不把 refresh token/注册私钥/业务私钥写入文件或 Web Storage。优先由宿主官方 SDK 供 token；如果选择 Rust 接管 grant，需要另有内存 owner 及明确交接 ACK，不能复用文件 checkpoint 假装无持久化。换进程重新授权和开启远程，换公钥不静默沿用信任。

安装 ID 只是展示关联，按已验证 owner/product 限定，不授予权限。当前注册 fence 的安全退役设计必须证明旧请求不能复活，包含多次进程重建、并发接管和未知注册结果反例；设计完成前不得简单删除 fence、加 TTL 或提高硬上限掩盖累积。

首版安装展示关联来自经过验证的 `Device.bridges[].installationId`，不新增设备级安装字段，也不依据相同安装ID合并注册、代际或公钥。宿主可在当前账号/产品的完整目录内筛选候选，仍逐个验证安全身份；没有bridge的设备不推测安装关联。

## 4. 浏览器桥接与正式入口

JS 目录 SDK 保持只读协调权限。增加独立安全桥接扩展契约，由适配器提供已认证加密的双向流，业务授权仍由宿主验证。能力描述区分设备传输入口、业务端口和浏览器安全桥接，不互相替代。连接描述绑定用户、产品、设备、代际、公钥身份、地址版本和过期；不支持的能力明确失败。

地址策略默认仍为 Quick Tunnel；受信自有入口通过服务端固定允许策略配置，Rust/JS 同步验证。禁止凭据、任意重定向和不受信 origin；路径/端口支持须显式约定。中继只承载端到端密文，外层 HTTPS 不能作为业务端到端加密证明。浏览器适配器不自行发明密码协议，复用经确认的桥接实现或标准协议。

## 5. 目录、保留与交付

目录通知提供用户/产品/epoch 绑定的 revision、完整快照和 reset 语义；先交付有限轮询，再实现受 Functions 时限约束的长轮询。信令 cursor 不作为目录 cursor。协调不可用应显示 unknown。

元数据清单须列出会话/设备/注册幂等/fence/seed/信令/去重/限流/等待者的用途、逻辑到期与物理清理。当前共享 seed 是协调秘密，不是业务解密密钥。安全退役与运维删除不混用；重置 namespace 是明确的协调重置。

发布兼容矩阵、RAM-only 样例及失败矩阵。SDK 原代码的分发授权需由所有者明确，实施过程不擅自修改 LICENSE；第三方原许可保持。
