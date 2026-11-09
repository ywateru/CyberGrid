# Redis 共享协调独立评审

当前结论：**PASS：源码、独立多实例 Redis 反例、Functions 预览、实际 Docker CI 及正式域名有限后验均通过。** 本结论不把健康检查成功当作完整认证或公网业务传输验收。

## 源码与契约

实现基线为提交 `9488018`。独立核对了冻结清单内 18 个文件的工作区字节、SHA256 与提交内容，均一致；评审同时对照 `docs/redis-coordination.md`、`docs/architecture.md`、`docs/deployment.md` 和服务配置说明。

生产入口创建 RedisStore，无 RAM 回退或本地 flock 权威。Store 的状态方法异步执行；共享 epoch、注册回执派生材料、revision、信令序列和配额由 Redis 管理。所有操作使用非只读 EVAL，Lua 在提交处重新检查有效会话、设备代际、请求 epoch 和相应容量。实体记录与到期索引分开保存，不以整份内存快照替换数据库。

已核查以下安全边界：

- 注册提交保留请求原始 epoch，避免并发刷新 Store 后把旧 epoch 注册提交到新 namespace 代际；注册 fence 在 namespace 生命周期内保留，受明确容量限制。
- 权限读取和业务变更走同一原子脚本，Redis TIME 决定租约与到期。过期索引通过请求内原子清理回收，不依赖 Functions 响应后的后台任务。
- Lua 类型检查及可拒绝的容量检查先于业务变更；脚本原子执行不等于运行错误后的事务回滚。
- waiter 使用共享、有期限的 token-owned lease；关闭实例只释放其连接，不删除共享状态。
- 全局硬上限为 512 个会话、256 个设备、1024 条／2 MiB 信令、2048 个注册 fence、4096 个幂等项、2048 个限流桶和 128 个 waiter。配置只能下调；锁内扫描不能通过配置放大到数百万记录。
- Functions 对 IP 入口限流采用保守共享桶，不信任客户端转发 IP；Node 使用 socket 地址。敏感状态、错误与管理投影不输出 bearer、派生密钥或提供商原始错误。
- `/healthz` 检查实例与 Redis 就绪状态，Redis 不可用返回 503；静态健康语义已与文档同步。托管 Redis 与 Logto 配置只在服务端使用。

## 独立定向验证

使用真实 Redis 7.4.3、独立测试 namespace、多个 Store／连接及两个实际 loopback HTTP handler，完成 **21 项独立断言，全部通过**。身份与应用目录是测试夹具，不代表真实 Logto 登录验收。

验证覆盖：

1. 注册已提交但回执丢失时，另一 worker 使用原幂等标识恢复相同会话与代际，且不会增生会话；接管后的旧 revision 不能复活。
2. 旧请求等待应用目录期间发生接管，晚到的旧快照返回 401，不能修改新代际的端口或发布 revision。
3. 调用者极端时钟不改变 Redis 决定的会话到期；24 个跨 worker 并发限流请求仅有配置允许的 3 个成功。
4. 身份校验等待期间重置 namespace，旧 epoch 注册返回 409 且不创建会话；已预热 handler 的配置随后返回新共享 epoch。
5. waiter 已领取但回执丢失时，只留下有期限的租约；到期后无需后台任务即可回收，后续正常取消释放自身租约。
6. 实际健康路由在 Redis 就绪时返回 200，部分元数据丢失时返回 503，并保留旧身份记录供故障处理，不误建新权威状态。

验证结束后测试 namespace 无残留键，两个 HTTP listener 与所有测试 Store 连接均已关闭。未修改用户 App、凭据或生产状态。

开发方另报告全服务 80 项、最终定向 13 项、JS 实际 HTTP 集成 1 项通过，以及托管 REST／TLS TCP 验证。本次未重复运行这些套件，也不将开发方结果冒充独立运行。

## 一致性与运行限制

Upstash 官方说明其复制模型为最终一致性，单 TCP 连接可提供因果一致性；原强一致性模式已弃用。普通 EVAL 的原子校验与主节点写路由用于正常请求路径，不能据此声称网络分区或故障切换时具有线性一致性。更强的故障保证必须由所选 Redis 服务实际提供。

免费 Redis 有存储、命令与带宽额度；有界轮询仍会消耗额度，不能承诺无限持续在线或始终免费。配额耗尽必须明确失败，不能建立第二份内存权威状态。Docker 的持久卷与 AOF 也不能替代备份或保证断电时不丢失已确认写入。

当前已验子集无未解决阻断项。生产采用独立 production 构建与配置，未直接提升预览并假定环境重新投影。

补充文档与许可检查：Go SDK、SDK 总览、Lab 与架构文档中的旧 RAM／owner 重启描述已同步为 Redis namespace 语义，Functions 共享限流桶例外也已明确。服务 lock 中 39 个精确依赖版本与 notices manifest 集合一致，39 份许可文本 SHA256 均匹配；6 个 Redis 包的缺失 npm 原文已按各自固定上游提交补齐。Docker 的构建上下文和运行阶段均保留 notices。

## Functions 预览增量

部署源码基线为 `dff3c14`，Redis 逻辑仍为 `9488018`。已独立核对根部署配置、Function 入口与 Redis 关键源码同预览上传源一致。所有路径仅内部 rewrite 到 Node Function，没有旧主机、隧道或外部 HTTP owner 路由。Docker 的唯一许可增量为复制 notices，并允许其进入构建上下文；运行阶段继承完整应用目录。本轮未重复构建 Docker 镜像。

独立通过平台登录保护旁路读取预览：Redis 就绪健康路由返回精确 `ok` 内容及 200，未知路径返回 404，两者均为 no-store。另核对开发方实际响应字节，文档及管理页与提交源码一致；首页仅多出平台预览工具栏脚本，精确移除此单一平台注入后与源码一致。开发方实际 Native 配置返回 authorization_code，并保持固定 resource；无真实用户身份的 JSON POST 返回固定身份错误，证明请求体到达 Function 内部身份校验，而非被平台预先吞掉。

同源 Vercel CLI 65.12.1 实际构建输出为 Node 22 Function。41 个 notices 文件通过构建器 `filePathMap` 引用进入函数，逐项 SHA256 与提交及上传源一致；独立核对 CLI 的映射还原逻辑确实加载源文件为 FileFsRef 并加入函数文件集合。因此未把函数目录缺少物理副本误判为遗漏，也未把远端不可展开的 opaque 函数对象冒充已下载检查。此为同源实际构建输出、映射实现及远端构建配置绑定的等效交付证据。

独立在内存解析实际拉取的预览及生产环境：两者均只配置 TLS TCP Redis，namespace 均显式存在且不同；固定 resource 保留 `/api`，生产 origin 保持正式地址。证据仅记录布尔结果，不输出连接地址、令牌或 prefix 值。生产必须重新构建，不能直接假定预览部署持有生产 namespace。

最终服务器 CI [42589602334](https://github.com/ywateru/CyberGrid/actions/runs/42589602334) 报告 82/82、0 skipped，许可证 39 项通过，实际 Docker Compose 验证非 root、Node 重启、Redis AOF 重启保持 epoch、Redis 停止时健康 503 及恢复。早期开发全套为 80 项；两者属于不同执行，未混记为独立重跑。后续提交 `8dcece4` 仅统一跨平台许可证来源元数据，Redis 固定上游许可证溯源保留，未改变运行逻辑。

## 正式域名有限后验

生产源码提交为 `8dcece43e28d3d401ae1d10b25962aefc8c91f1e`。独立将生产上传源清单中的 77 个文件 SHA256 对照该提交 blob，全部一致；部署方的 production 构建、正式 alias 与候选探测证据已核阅。部署仍使用同一内部 Function 路由，无旧主机外部 rewrite 依赖。

在部署方正式 READY 后，独立对正式域名执行且仅执行四个无凭据 GET，无平台保护 bypass：

1. 健康路由 200，内容为精确 3 字节 `ok` 换行。
2. Native 配置 200，clientId 与公开预置一致，loginFlow 为 authorization_code，resource 保持正式 `/api`，协议为 memory-v1；配置含 epoch 与已核生产候选一致。
3. 管理概览未提供凭据时返回 401、固定 INVALID_ADMIN_KEY。
4. 未知路径返回 404。

四个响应均包含 no-store 与平台响应标识，未创建用户会话或使用真实用户 token。至此关闭本任务的 Functions + Redis 有限运行门槛。未重复旧 SDK、原生、手机、登录或 WAN 传输套件；多 worker 竞争与故障反例来自前述真实本地 Redis 独立验证，而非声称在生产制造故障。上游 Redis 一致性、免费额度和远端 opaque 构建对象的证明边界仍按本报告保留。
