# 协调元数据、兼容性与分发边界

本文区分当前源码、已发布产物和计划能力；测试报告中的本地签名 fixture 不等同于真实 Logto 云登录。完整改进范围见 [覆盖表](plan/analysis/integration-progress.md)。

## 元数据与删除

Redis 保存协调元数据，业务正文、文件、终端内容和端到端解密私钥继续留在客户端。必要的网络地址、公钥指纹与设备名称属于协调元数据，不能把“不保存业务内容”写成“不保存任何客户端信息”。

| 实体 | 用途/敏感性 | 当前逻辑有效期 | 物理清理 |
| --- | --- | --- | --- |
| sessions | bearer hash、owner、产品、真实 client/mode、设备代际；具有关联隐私 | 24h，撤销/接管可提前失效 | 撤销主动删除；到期在后续请求原子清理 |
| devices | 名称、端口、地址、指纹、iroh 地址、代际与发布 revision | 可连接性受 session 限制；online 租约 65s | 撤销删除本 session 所属设备；旧协议 metadata 自注册/最后 publish 起保留 24h，再懒清理，可能晚于 session 到期 |
| registrations（legacy-v1） | 精确请求摘要、session hash、设备关联的幂等回执 | 相应 session 到期；接管可替换旧回执 | 接管可删旧回执；单独 revoke 不保证立即删除回执，到期懒清理，不能独立抹去未知操作事实 |
| fences（legacy-v1） | 已用注册键、最大 revision、原 request/content；防止旧请求复活 | namespace 生命周期，无自动 TTL | 不能因 close 或空闲而删除；受用户/全局上限限制 |
| finite-v2 准入域/fences | 最新签发域、注册 revision、全部已签票据最晚到期；防止旧域复活 | 全部已签 ticket 到期且无有效关联 session 前必须保留 | 满足退役条件后请求触发原子回收；全局签发序列不随单个 fence 回收重用 |
| finite-v2 mint/register 回执 | 绑定真实身份/产品/key/epoch 的幂等操作事实 | 保留至对应 ticket/session 绝对到期，ticket 最长 24h | 共享幂等配额；接管不能提前移除旧 register 回执，过期后请求触发清理 |
| signals / dedup | 信令 payload 和去重摘要，可能包含候选地址；不是业务内容 | 120s | 请求触发到期清理，设备撤销/接管同步清关联信令 |
| rates | 鉴权/API 限流 key 的 hash 和计数 | 60s 窗口 | 请求触发懒清理 |
| waiters | 长轮询占用和设备关联 | 最多 17s 的租约 | 请求 finally 释放；异常退出后到期懒清理 |
| meta / initialized / revision / sequence | namespace epoch、共享幂等派生 seed、计数与限制 | namespace 生命周期 | 只由明确运维重置退役；seed 是中央协调秘密，不能公开或当成业务私钥 |

以上清理不依赖 Functions 响应后的后台任务；无后续请求时物理记录可能仍在 Redis，不承诺在逻辑到期的同一时刻自动擦除。Redis 持久化、备份、副本与提供商日志的保留另受部署者/provider 策略约束。

服务端已提供可选 [finite-v2 准入](registration-admission.md)，默认仍为 legacy-v1；Rust SDK 的有限准入协商已在未发布源码及真实 Node/Redis/JWK 本地链中验证；生产迁移及匹配客户端发布尚未完成。有限模式的 session 与 device 记录不能超过 ticket 绝对到期，publish 不能延长；不能用其 24h 回收规则解释旧永久 fence。新模式要求独立 prefix 与旧部署退役；不通过重置整个 namespace 掩盖容量累积。namespace 重置会让全部协调 session/设备/信令失效，客户端按新 epoch 重新注册；不会删除 Logto 用户或端上业务数据。

Logto 账号退出、撤销 refresh grant、从产品允许策略移除 client、关闭本机业务 gate、撤销 CyberGrid session 和运维删除元数据是不同动作。当前服务没有提供自动的跨 Logto/Redis 账号删除事务，也没有通用管理员按 owner 擦除 API；不能声称删除 Logto 用户会立刻清空 Redis。需要合规删除时，由部署者停用相关授权/旧实例并执行经过审查的定向清理与 provider 备份流程，不能在线随意删 fence 破坏防重放。

## 能力与平台

| 范围 | 当前可用/验证范围 | 限制 |
| --- | --- | --- |
| Node / Docker / Vercel Functions | 同一服务实现及 Redis 原子协调；实际多 handler Redis 测试 | Functions 不承载常驻业务连接，端到端内容不经中央服务 |
| Vercel KV / Upstash | 支持 KV REST 变量与 Upstash REST/TLS TCP 选择，执行写路由 EVAL | 免费额度及提供商故障模型不是无限资源/网络分区线性一致保证 |
| Rust native | WebRTC、iroh direct、Quick Tunnel 内层 mTLS；已知 macOS arm64 / Android arm64 宿主 | 其他 OS/架构未由当前实机证据覆盖；不把能编译推断为已验收 |
| JS console | 宿主官方 Logto 登录后只读应用、设备和显式端口地址；RAM-only 协调状态 | 不注册 Native 设备，不获得设备信令或通用 TCP 权限 |
| 浏览器桥接 | 服务端目录与 JS gated wrapper 已实现于未发布源码，交付状态按任务记录 | 已有私有本地实际 Noise 互通证据；不打包外部实现，也不代替完整样例、真实云登录、手机或 WAN 验收 |
| 手机浏览器 | 作为独立桥接目标验收 | Android 原生 SDK 结果不能替代 Chrome/Safari 验收 |
| Go / 旧版 Lab 下载 | 原有版本保留原协议和已发布能力 | 未自动获得新 Rust gate、静态产品/准入能力；必须按实际组合验证 |

公开配置是能力协商的入口：仍须先固定可信 server/issuer/client/resource，不能用服务器返回的新 client 替换宿主固定 client。未知 required 能力要明确拒绝，不能降级到权限更宽的方式。static 产品模式保留真实 OAuth appId，但其设备命名空间为 canonical product；应用目录 `source` 可能为 static 或 logto。会话绑定 mode/client 后，换模式或 client 移组需要重新授权；只更新允许策略的部分 worker 不能即时撤销仍在旧公开部署上的会话。

首次新增默认 remote gate 的 Rust SDK 是行为变更：宿主必须显式 enable，不能只升级依赖后宣称旧 Lab 已完成集成。有限准入的缺票客户端将被明确拒绝；部署时必须配套 SDK、文档和测试，先验证再切换生产，不把新源码当作已更新下载包。

## 授权窗口与责任

当前 access token 在 exchange 时验签，协调 session 独立最多 24h；SDK 获取新鲜 token 的回调不是实时 Logto 撤销机制。已签发 token 的有效期、协调租约和业务短期授权各自独立。宿主必须在业务层执行其授权续期 ACK、到期和拒绝后的关闭；CyberGrid 不解析工作区业务权限，也不自动重放业务写操作。

持续 reauthorization 的服务/SDK 策略仍是后续验收项；不能把新 minValidity/forceRefresh 字段或 30 分钟复用同一个 session 的测试描述成已覆盖连续撤销。断网本机关闭由本机 gate 保证，云端撤销确认独立返回；远程端可能在下一次目录/连接检查前继续显示旧缓存，不能据此反向开放本机。

## 嵌入与分发许可

项目自身当前 `LICENSE` 保留所有权利；公开源码与可下载构建物不自动授予第三方嵌入、修改或商业再分发权。本次改进不擅自更改该许可。接入者应在分发包含 CyberGrid SDK 的产品前获得所有者明确授权；所有者若决定采用独立 SDK 许可，应单独修改正式 LICENSE 和发布说明，而不是只改 README。

第三方依赖保持各自原许可和 notices。AnyWork 的 AGPL 实现只作为用户已有外部宿主进行互通测试，不复制、打包或隐式加入 CyberGrid 依赖树；测试前记录其确切版本及授权边界。任何桥接 adapter 的公开发行需再次核对实际依赖和授权，不能用现有 MIT/Apache 依赖的许可代替其他代码的许可。

## 仓库验证与发布边界

完整仓库 CI 使用真实 Redis、Node 依赖及 `REQUIRE_REDIS=1`，执行 Rust `cargo test --locked --manifest-path sdk-rust/Cargo.toml -- --include-ignored`、JS 单测及新旧 HTTP 集成，以及 RAM-only 模块的官方 owner、loopback 测试和构建。独立 Rust 源码包不含服务端/Node/Redis；默认 ignored 的外部依赖测试不是完整仓库通过。公开 CI 不下载私有外部 AnyWork，受控生命周期 fixture 也不是实际 Noise 证明。新增 workflow 的定义或本机成功不能写成 Actions 已执行；实际运行结果另行绑定提交记录。

发布前由最终冻结重新导出、扫描、核对许可与产物。当前 tree 的检查不覆盖 Git 历史；有本机路径的未发布历史不能随整条分支推送后宣称隐私已清理。允许从审过的公开快照生成发布提交并记录 source/tree/commit 映射，不改写已发布历史。012 的中间快照不覆盖后续 010 新文件。

## 下一版本准备（尚未发布）

项目目标 v0.8.0：Rust SDK 0.3.0、JS SDK 0.2.0、Lab 0.4.0、Node server 0.2.0、首次 RAM-only 样例 0.1.0；Go 历史系列保留 0.4.0。Rust 默认 remote gate、fresh-token/finite 准入与 JS directory/watch/adapter 行为应由匹配宿主消费，不能仅升级依赖后沿用旧行为假设。Lab 与 RAM-only 样例是不同交付物，公共样例默认没有外部 Gateway factory/Noise adapter。完整仓库源码包保留 server/SDK/样例相对路径，不另发缺失相对依赖的样例 ZIP。

这些版本和归档名称是发布准备，不表示 GitHub 产物、生产迁移或完整样例验收已完成。既有 v0.7.0（Rust 0.2.0 / JS 0.1.0 / Lab 0.3.0）的内容保持不变；历史 Go 设备码及旧客户端不获得 finite-v2 能力。真实云、公开隧道、手机和长跑状态仍按 integration-010 单独验收。
