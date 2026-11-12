# integration-012 独立评审

结论：**BLOCKED**。冻结 commit `b605ba9f2901d3828eae94cd228b666f58e62082` 存在一项 **P1 blocking：I012-P1-01**。应用目录已因权限拒绝进入 unknown 后，较早设备请求的迟到成功仍可恢复“在线”。现有用例、实际打包和 HTTP 集成通过不能抵消该反例。

## 冻结绑定与范围

- 分支 `task/integration-012`；parent `5dca05282499e8ee9fd05c61e12acb923c5c0d07`，原 base `e4e4c1b`。parent 为 root 已认可的公开文档隐私修正，与 root 的 `32f2561` 等价；任务本身为其后的 19 路径，不将 parent 文档差异混入任务改动。
- 按 root 当前 `docs/INDEX.md`、`docs/plan/tasks/integration-012.md`、`docs/common-sample.md` 及部署、桥接、集成、兼容性和发布合同审查，采用 docs-sprint 独立 verify 角色。
- 19 路径全部审查：四份 workflow，兼容性/发布文档，六项 release 策略/打包脚本与 policy，两份 console 页面壳、实际 console 脚本、主页/静态文档、网站构建入口和 console 测试。source hash 全部匹配开发冻结清单；开发 `SHA256SUMS` 的 50 项全部实算匹配。
- 独立证据标签 `integration-012-independent`；`INDEX.md` 为索引，`commands.json` 记录实际命令、cwd、exit，`freeze-binding.json` / `freeze-after.json` 绑定冻结状态，`counterexample-binding.json` 保留原反例字节 hash 与 exit 1。机器路径、原始证据 hash 和 fixture 数据仅留私有 archive。
- 未修改开发实现、锁文件、外部实现或生产。三项临时依赖 symlink 已移除；结束时冻结树 clean、HEAD 未变，无运行中的验证进程。本轮未使用 Cargo target，也未 clean 或创建 worktree。

## I012-P1-01 — P1 / blocking：新应用目录失败后，旧设备响应可恢复在线

合同：`docs/plan/tasks/integration-012.md` verification 2 要求网络/权限/解析/容量错误先撤销可用结论、显示 unknown，并阻止并发读取的旧响应覆盖当前上下文。`docs/common-sample.md`「远程 Web 授权与页面」要求目录错误先清除可用结论；`docs/browser-bridge.md:69` 要求失效/不可达时 unknown，不能继续以旧缓存声明 ready。此 console 不承载业务 wrapper，本 finding 指其公开的目录/在线结论，没有把它描述成业务连接。

代码：`server/public/console/console.js:55` 的 `loadApps()` 只推进 appsRevision 并取消 appsAbort；`:59` 与 `:68` 虽然显示 unknown，却没有使此前设备读取的 deviceRevision 失效。`:65` 仅在成功取得新目录且所选产品改变时取消设备请求。与此同时，`:73`—`:75` 的设备 guard 只看 generation、selection、产品值和 deviceRevision；应用目录失败不改变这些值。旧设备响应因此能通过 `:79`、`:82` 的检查，在 `:83` 重新写入在线计数。

独立最小复现：

1. 执行冻结的实际 `console.js`。通过其 `beginLogin()` 生成 PKCE/state/nonce，再经本地 HTTP、真实 ES384 ID token/JWK 验证及实际 callback 控制流取得 console fixture 会话。没有直接 seed 内部 session 或改写源函数。
2. 正常显示所选产品的一台在线设备，发起设备刷新 A，并扣住该 HTTP 响应。
3. 发起较新的应用目录读取 B；实际 HTTP 返回 403 FORBIDDEN。B 拒绝，页面正确变为“状态未知”。
4. 放回 A 的成功响应。页面重新显示“1 台在线，共 1 台终端”，覆盖了 B 已确认的 unknown。

`console-cross-request.test.mjs` 的合同断言实际失败，**0 pass / 1 fail，exit 1**；`console-cross-request.log` 记录上述在线计数，原 harness 字节已绑定，后续 r2 应原样执行。HTTP fixture 故意允许已取消请求迟到完成，验证的是实际脚本 guard 的必要性，不依赖 abort 一定阻止响应。

建议修复：将应用目录与设备目录纳入同一用户/产品有效性边界。应用目录重新验证或失败时，必须让此前设备读取失去提交资格；仅清 DOM 或取消 apps 请求不足以阻止迟到恢复。保留当前固定错误文案、同产品最新请求提交、切产品/注销 fencing，不通过重新接受旧响应来消除 unknown。

## 独立实际检查

| 验证 | actual 结果 | 私有证据 |
| --- | --- | --- |
| 新写的跨应用/设备请求反例 | 1 fail，exit 1，失败为合同断言 | `console-cross-request.test.mjs`、`console-cross-request.log` |
| 既有 shipped console 用例 | 24 pass / 0 fail，exit 0 | `shipped-console-existing.log` |
| 实际网站/服务 TypeScript build | exit 0 | `server-build.log` |
| 构建后 ESM → legacy 实际 HTTP | 1 pass / 0 fail，exit 0 | `js-legacy-http.log` |
| 构建后 ESM → 双实际 handler / ES384 JWK / 真 Redis | 1 pass / 0 fail / 0 skip，exit 0，REQUIRE_REDIS=1 | `js-redis-http.log` |
| 冻结实际 package.py sources | exit 0；只把输出目录重定向至私有证据 | `package-sources.log`、`package-sources-private-output.py` |
| 归档 ESM 实际导入与 provenance | exit 0；八 dist 文件、逐文件 hash、source manifest 与冻结 commit 一致 | `package-proof.json`、`package-esm-import.log` |
| 既有 source policy 测试 | 5 pass，exit 0 | `policy-existing.log` |
| 独立实际 policy 负控 | wrapper exit 0；planner/exporter/scanner 按预期拒绝，原输入保留 | `policy-independent.py`、`policy-independent-proof.json` |
| 四份 workflow 的实际 YAML 解析与配置核对 | Ruby/Psych 全部 exit 0，断言通过 | `workflow-parsed.json`、`workflow-config-proof.json` |

独立 policy 负控使用冻结的实际脚本：未审 sample binary 使 planner exit 1；伪造 keep plan 仍被 exporter 拒绝；scanner 对 `.DS_Store` exit 1 且不删除原输入；对完整 2815 文件计划，在实际复制时注入额外输出文件，exporter 以 inventory 不符拒绝，未产生 ready 报告。完整计划中的冻结输入 hash 前后全部一致。没有把“测试 wrapper 按预期处理拒绝”的 exit 0 写成被测负例自身成功。

开发报告的 server 137、JS 59、Rust 78、sample Rust 17、owner 5 等只审计日志/hash，不充作本轮独立计数。发现 P1 后按 root 要求完成必要打包/配置检查，没有重复不受改动的大套 Rust/sample、云登录或 Noise 验收。

## 打包、网站与 workflow 审查结果

`scripts/release/package.py:96` 精确列举 index、bridge、directory、endpoints 的 `.js` / `.d.ts`，`:98` 拒绝额外、缺失、symlink 或非普通 dist 输入。独立执行实际 CLI 实现，仅重定向私有输出以保留开发产物；实际 ESM 归档恰含八文件，所有 BUILD-INFO hash 及 source manifest 对应冻结源码，实际 import 后可创建 Client 并取得 directory/watch/connectBridge 入口。归档 hash 与开发同冻结的确定性输出相同；没有 node_modules 或外部 AnyWork 源码。

新增 sample binary 例外仅为精确 icon 路径，没有把整个 sample root 放入 binary allowlist；原 provider/build-tool 例外仍保持原范围。exporter 按当前 policy 再检查 plan，核最终 inventory；scanner 是 literal/file-class 扫描，不是完整源码或历史批准。开发已有当前快照的导出/扫描证据；本轮独立负控不替代最终 010 新源码的重新导出、隐私和许可检查。

主页、静态 docs、README 部署按钮均推荐 `CYBERGRID_PRODUCTS` 静态策略；Management M2M 仅用于未设静态策略时的可选动态模式，public origin 保持可选。按钮 env 集与实际部署合同一致；console 和 callback 页面壳逐字一致。网站明确把静态产品、finite admission、gate、JS bridge 和 RAM-only 模块标为未发布源码能力，未把它们写进不可变 v0.7.0 下载承诺；旧 Lab 使用真实 Native client 与新 canonical product 的区分保留。

workflow 配置满足本任务定义的检查入口：

- integration job 安装真实 Redis 和固定 Node/Rust，使用 `npm ci`、`REQUIRE_REDIS=1`，Rust SDK 与 sample 均显式 `--include-ignored`，包含官方 owner、实际 loopback 模块的 test/build/clippy。
- JS workflow 与 release sources job 均安装 Redis、运行 JS 单测及新旧两条 HTTP 集成；本轮两条 HTTP 已实际执行，Rust/sample 完整命令仅核对配置及开发证据。
- 触发范围覆盖 server、两 SDK、sample 和 release policy；五项当前锁与 parent 字节相同，integration 有最终锁漂移检查；publish 显式依赖 integration 与三项打包 job。
- Web 固定路径为 `examples/ram-only-workspace/web`，存在 package.json 时依次 npm ci/test/build。**本冻结该目录尚未交付，结果为 PENDING，不能记 Web PASS**；010 最终必须存在并重新验证。
- 公开 CI 没有拉取私有 AnyWork 的命令；没有把受控生命周期 fixture 或 CI 编译描述成实际 Noise、Logto Cloud、WAN、手机或长跑完成。

本轮没有启动 GitHub Actions、Docker/Android 发布、真实浏览器页面/云登录、生产、手机、push 或部署。console 反例使用实际脚本、最小 DOM 壳和本地签名/HTTP fixture，验证真实控制流和响应次序；它不是完整浏览器登录或平台兼容性验收。本机 Node 与配置中的固定 Actions Node 版本分别记录，未声称本机执行等同 Actions 成功。

## 交付门槛

**BLOCKED：先由开发 owner 修复 I012-P1-01，冻结新 commit 后原样复执行公开登录链反例，并核对受影响的应用/设备交叉请求、产品切换、退出与失败 unknown。** 其他通过证据可按精确 hash 继承；010 最终源码、Web 与发布快照仍须另行冻结和验收。
