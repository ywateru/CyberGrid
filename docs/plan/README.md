# Redis 与 Functions 交付

## 产品集成改进

用户已授权按 [反馈评估](analysis/anywork-integration-2026-11-10.md) 实施改进。设计来源为 [产品集成契约](../integration-contract.md)。整体目标包括产品绑定、授权/状态、关闭/身份、浏览器安全桥接、正式入口、目录通知及共同样例；下表首先启动可独立验证的服务端与 JS 工作，后续工作随设计细化追加，不将前两项完成视为整体完成。

| 任务 | 范围 | 状态 |
| --- | --- | --- |
| [integration-001](tasks/integration-001.md) | 产品绑定、静态允许配置、真实 client 及命名空间隔离 | done |
| [integration-002](tasks/integration-002.md) | JS token 新鲜度、状态快照、错误与重试 | done |
| [integration-003](tasks/integration-003.md) | Rust 授权/状态、显式开启与本机关闭远程 | done |
| [integration-004](tasks/integration-004.md) | 有限期注册准入、原子时限与安全 fence 回收 | done |
| [integration-005](tasks/integration-005.md) | 服务端受信入口、bridge 元数据与版本化完整目录 | done |
| [integration-006](tasks/integration-006.md) | Rust 准入、入口能力与 bridge 发布接口 | done |
| [integration-007](tasks/integration-007.md) | JS 目录 watch、正式安全桥接及真实宿主互通 | done |
| [integration-008](tasks/integration-008.md) | Lab 显式远程开关、分层状态与 ADB 控制 | done |
| [integration-009](tasks/integration-009.md) | 公开源码策略补齐集成文档、导出与隐私检查 | done（冻结基线，非最终发布检查） |
| [integration-010](tasks/integration-010.md) | 共同样例、手机浏览器/长跑与最终发布验收 | in-progress |
| [integration-011](tasks/integration-011.md) | RAM-only桌面owner、本地Web及独立Gateway上游模块 | done（模块） |
| [integration-012](tasks/integration-012.md) | 网站/console、静态部署说明及真实仓库CI | done（模块） |
| [integration-013](tasks/integration-013.md) | 新Web CI、版本兼容与最终源码归档准备 | done（准备模块，非正式发布） |

001/002 已经独立评审并合入；根分支复跑服务端 **97/97**（真实 Redis、0 skip）、JS **31/31** 及真实 HTTP 集成 **1/1** 通过。独立验证另用实际 HTTP/Redis/JWK 验签检查 12 个产品/隔离/旧回执场景，以及新 JS 构建与服务组合的 4 个场景。签名 fixture 不代表真实 Logto 云登录或公网验收，生产未变更。

003 经独立反例复核修复两个首次恢复问题后合入，根分支 Rust **60/60** 通过。独立新增 4 项真实流/恢复反例与 13 项定向复测通过，细节见 [评审](reviews/integration-003.md)。Lab 尚待显式接入，不将源码 SDK 通过视为已发布客户端通过。

004 已合入并通过根分支全服务 **107/107**（真实 Redis、零跳过）；独立最终九组验证PASS，细节见 [评审](reviews/integration-004.md)。服务端可选 finite-v2 已实现，默认仍为 legacy-v1；Rust 客户端接入和生产迁移尚待完成，不能据此宣称已解决线上 RAM-only 注册容量。

005 已合入 `851fa17`，独立15项实际Redis/HTTP/ES384场景PASS；根分支完整服务 **123/123**（真实Redis、零跳过）、JS实际HTTP集成 **1/1** 和server build/typecheck通过，见[评审](reviews/integration-005.md)。服务端已提供受信入口、bridge发布及分域完整目录，两个SDK的接入与真实浏览器业务仍待完成。

008 replacement冻结已独立PASS并合入 `1da15cb`；根分支host **28/28**、Python **22/22**、前端构建/状态断言及browser-start **2/2** 通过。独立7项实际host场景覆盖优先关闭、live状态、未知操作所有权与双向mTLS流关闭/本地继续，见[复审](reviews/integration-008-r2.md)。首次BLOCKED报告保留为历史；实机/云登录/WAN仍待验收。

006修复冻结`d952d25`的[独立复审](reviews/integration-006-r2.md)PASS，已合入`26af081`/`cd3e891`；根完整Rust **78/78**（实际执行ignored集成、零跳过）、最新Lab宿主含ADB **28/28**及消费者locked构建通过，旧base断言在当前组合通过。007修复冻结`a1d891c`的[独立复审](reviews/integration-007-r2.md)PASS，已合入`d081839`/`0465e75`；根分支完整JS **59/59**、旧HTTP **1/1**及真实Redis新链 **1/1**均通过，原BLOCKED保留。

011修复冻结`ee96a17`的[独立r2评审](reviews/integration-011-r2.md)PASS，已合入`7ee1acf`/`f3b3a05`；根组合workspace Rust **17/17**、官方owner JS **5/5**、前端和原生locked构建通过。默认factory仍不可用，不能代替010实际桥接；010公共样例与源码树外实际Gateway装配、012站点/console/CI已分路径进入实现，010最终发布仍受012 release gate约束。

012修复冻结`44cc631`经[独立r2复审](reviews/integration-012-r2.md)PASS，已合入`2ce0dc1`/`e97ad1e`。根实际服务**145/145**、JS**59/59**、新旧HTTP各**1/1**、SDK Rust**78/78**（include-ignored，零跳过）、现有样例**17/17**、owner**5/5**与build/clippy/policy通过。原跨目录迟到响应P1已关闭，原BLOCKED保留；012模块通过不代表Actions已运行或010新Web/真实桥接与最终导出已验收。

010公共模块修复冻结`141dcd1`经[独立r2复审](reviews/integration-010-r2.md)PASS，原两项P2关闭，合入`7daad58`/`b0831ca`/`652634a`。根合并后实际样例Rust**22/22**（include-ignored，零跳过）、Web真实仓库集成**8/8**自然exit0、Web/desktop owner各**5/5**、两前端build及Rust build/严格Clippy通过；样例内容与独立冻结完全一致。013已满足启动条件，010整体仍为in-progress，不将fixture当真实Logto/Noise/手机/长跑证据。

## 已有协调服务交付

本轮将单进程 RAM 协调迁移至 Redis，同时支持 Node / Docker 和 Vercel Functions。用户已授权实现、提交与部署，并明确允许 Functions；旧服务器仅在新服务验收后退出生产路径。

实现、部署与独立验收分工见任务文件。设计来源为 [Redis 协调契约](../redis-coordination.md) 和 [部署](../deployment.md)。

| 任务 | 范围 | 状态 |
| --- | --- | --- |
| [redis-001](tasks/redis-001.md) | 异步 Store、原子 Redis、Node / Functions、Docker | done |
| [redis-002](tasks/redis-002.md) | Vercel 构建、免费 Redis、预览与正式域名 | done |
| [redis-003](tasks/redis-003.md) | 独立多实例、重启、竞争与线上路径验收 | done |
| [wan-001](tasks/wan-001.md) | Wi-Fi / 蜂窝实机验收，通过后同步线上与 GitHub | in-progress |
| [kv-001](tasks/kv-001.md) | Vercel KV 变量接入、文档及生产配置 | done |

本轮不改变端到端传输协议，不重复构建已发布原生客户端。Wi-Fi / 4G 的实机传输验收独立记录，不能用 Redis / CI 通过代替。

源码基线 `9488018` 已通过独立复核与 21 项真实 Redis 反例检查；开发方全服务 80 项、最终定向 13 项、JS HTTP 集成以及托管 Redis 的 REST / TLS TCP 检查均通过。[服务 CI](https://github.com/ywateru/CyberGrid/actions/runs/42589602334) 在提交 `8dcece4` 通过：82 项测试、0 失败、0 跳过，另含 39 个依赖的许可证核验和 Docker Compose 非 root / AOF 重启 / 不可用恢复检查。线上部署结论见 [评审](reviews/redis-003.md)。

2026-11-09，[正式域名](https://cybergrid.aixgg.com/) 已部署运行源码 `8dcece4` 的 Vercel Functions，使用生产 Redis namespace。预览和生产的 namespace 隔离已核对；正式域名的公开有限检查通过，包括健康、登录配置、未授权管理访问拒绝与未知路径。此结论不包含新版本的真实 Logto 用户登录和 Wi-Fi / 4G 端到端传输验收。
