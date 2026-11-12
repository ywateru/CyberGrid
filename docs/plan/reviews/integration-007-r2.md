# integration-007 r2 独立复审

结论：**PASS**。冻结 commit `a1d891ce04e9f409316a21f689095db846028bfa` 关闭原评审 R1、R2 两项 P1 blocking；本轮未发现新的 blocking 或 non-blocking finding。此结论仅适用于下述冻结绑定与验收范围，由 root 决定合入。

## 冻结绑定与审查范围

- 工作树：`task/integration-007`，分支 `task/integration-007`；parent `b5acbec4586beb7271616ab150bfcf084bd4e447`，原 feature base `eae29328995696fb741b18f4ab993d3048623fec`。
- 采用 root 当前 `docs/INDEX.md`、task、`browser-bridge.md`、`integration-contract.md`、`metadata-and-compatibility.md`，按 docs-sprint 独立验证角色执行；合同没有为修复放宽。合同 hash 与 root HEAD `0589f8bf4a0275439c321e1d46e88bc5ccf5ca6c` 见私有 `root-contract-binding.json`。
- 完整审查 r2 四路径 diff：`sdk-js/README.md`、`sdk-js/src/index.ts`、`sdk-js/test/bridge.test.mjs`、`sdk-js/test/bridge.integration.mjs`。其余四个 feature 路径与原评审相同；原八路径源码审查结论保留，不声称本轮重新执行原有全部边界测试。
- 八项 feature source SHA256、独立构建后的八项 dist SHA256 全部匹配 r2 开发 `source-final.json`。开发 `SHA256SUMS` 21 项实算匹配，四份最终日志 hash 匹配；开发单次完整 59 PASS 仅作为开发证据审计，不计入独立测试数量。
- reviewer 没有修改实现。临时链接根目录已有 `sdk-js/node_modules`、`server/node_modules`，验证后均已移除；最终冻结工作树 clean，HEAD 未变。
- 私有证据目录：`private-evidence:integration-007-r2-independent`。命令、cwd、actual exit 与耗时见 `commands.json`；绑定见 `freeze-binding.json`、`freeze-after.json`，证据索引及校验见 `INDEX.md`、`SHA256SUMS`。

## 原 finding 复验

| Finding | 原 severity / blocking | r2 结果 | 合同与代码 |
| --- | --- | --- | --- |
| R1：可信配置移除已选产品后继续接受旧产品 | P1 / blocking | **CLOSED** | `integration-contract.md:26`；`index.ts:138` 保留经验证产品集合，`:262` 校验所选产品，`:239` 在恢复后的请求重放前校验，`:265`、`:278`、`:296` 在异步边界后复核 |
| R2：完整目录 unknown 发出时旧业务 gate 尚开放 | P1 / blocking | **CLOSED** | `browser-bridge.md:69`、`:77`、`:81`；`index.ts:254` 将完整目录失败置 unavailable，`:118` 在状态观察者和 watch unknown 之前同步执行 invalidators |

先原样复执行原独立反例的前两项合同断言（不包含已排除的本地 2MiB 错误码假设）。本轮 `original-counterexamples.log` **2 PASS、actual exit 0**：产品从 Core 移除后明确拒绝；畸形完整目录的 unknown 回调看到 `phase=closed`、`reason=directory-unavailable`、`cleanupPending=true`。原 b5 的 exit 1 证据和 BLOCKED 报告保留，不改写历史结果。

修复审查确认产品集合由完整可信配置解析产生，建立会话和策略 reload 均保存；所选 product 不被替换。检查覆盖请求前、恢复完成后的 GET 重放前、等待 reload 后及旧 in-flight 响应返回后，避免一次较旧响应覆盖已经获知的产品移除。针对用例实际确认恢复后的配置移除 Core 时没有再重放旧 Core 目录请求。

目录失效处理覆盖解析错误、权限拒绝、服务端 413 `DIRECTORY_TOO_LARGE` 和本地超大原始 body；gate 在状态观察者和 unknown 回调之前关闭，回调内 request/stream 无法派发。`CANCELLED`、本地 `INVALID_INPUT`、由有效设备快照查找不存在端口产生的 `NOT_FOUND` 保留原有效 gate。独立新增负控实际取消在途目录、随后放回忽略取消的迟到响应，再触发本地端口 NOT_FOUND，旧通道仍可正常业务请求；这没有把完整目录失败误当本地输入错误。

2MiB 仍按原准确合同判断：服务端 HTTP 413 固定码保留；畸形 HTTP 200 的本地 bounded reader 返回 `INVALID_RESPONSE`，不另造固定码要求。这两类不可用完整目录均会同步关闭 gate。

## 独立运行结果

| 验证 | actual 结果 | 私有证据 |
| --- | --- | --- |
| TypeScript build 与八项 dist hash | exit 0，全部匹配冻结清单 | `build.log`、`package-independent.json` |
| 原 R1/R2 独立反例 | 2 PASS / 0 fail，exit 0 | `original-counterexamples.test.mjs`、`original-counterexamples.log` |
| 修复相关测试的独立针对性执行 | 8 PASS / 0 fail / 0 skip，exit 0 | `targeted-fixes.log` |
| 新增独立取消、迟到响应与本地查找负控 | 1 PASS / 0 fail，exit 0 | `negative-controls.test.mjs`、`negative-controls.log` |
| built ESM → 双实际 HTTP handler → ES384/JWK → 真 Redis | 1 PASS / 0 fail / 0 skip，exit 0 | `redis-http.log` |
| legacy built ESM → 实际 Node HTTP / MemoryStore / epoch recovery | 1 PASS / 0 fail / 0 skip，exit 0 | `legacy-http.log` |
| 实际浏览器 clatterjs ↔ Rust snow Noise IK，结合真实 handler / Redis | 17 checks，exit 0 | `browser-noise.log`、`noise-browser-result.json` |
| npm pack dry run、依赖许可字段、diff whitespace | 正确包目录 dry run exit 0；依赖许可字段未变；diff check exit 0 | `package-independent.json`、`pack-dry.log`、`diff-check.log` |

单元/反例部分是三次执行的 **2 + 8 + 1 个测试用例**，其中行为覆盖有重叠；没有声称独立 full 59，也不把 Redis/legacy 用例和 Noise check 混入单元数量。针对性 8 项覆盖可信产品移除、初始产品选择、四类完整目录失败的观察次序、本地输入/显式取消，以及同策略 session recovery 的 replay 前校验。

实际 Redis 链验证 Native 发布、watch、失效，另验证真实 handler FORBIDDEN 在外部 unknown 前封闭通道，以及当前配置移除 Core 而旧目录仍能返回 Core 的边界。该链的 adapter 是生命周期替身；真实密码学结论来自另外实际执行的浏览器 Noise 链。

Noise 17 项保留原真实握手、错误 key/installation、短授权 ACK 续期与拒绝、在途业务中止、流读写、140000-byte 分块密文 echo、目录撤回、取消后迟到 peer 清理与 wire 不泄露业务 token；新增真实 handler 403 后的认证通道 gate 关闭、unknown 回调零业务派发、实际 WebSocket 关闭三项检查。echo SHA256 `717721f9f1f029e636862a903c88a00ea1cdd5c0d30942eb79533f44a7a1885e`，实际连接 6 次、续期调用 3 次、close 调用 4 次。

打包检查曾从仓库根用 `npm pack --prefix sdk-js` 得到 exit 1（根 package 无 name/version）。这是 reviewer 命令 cwd 错误，原日志保留为 `pack-dry-wrong-cwd.log`；改在 `sdk-js` cwd 执行 `npm pack --dry-run --json` 后 exit 0，产物清单无外部 AnyWork 源码或 node_modules。未因该命令错误修改产品。

## 外部绑定与验收限制

外部只读 AnyWork 当前实际 HEAD 为 `6d9cf651202fc32b0d081d030e84209be51946dc`，不同于开发清单记录的 `4f23c452b61c935720980cf715fa483844c0b507`。只读核对 old..new 仅四个无关文档路径；十项相关源码/manifest/lock hash 及已构建 binary hash 全部一致。root 明确允许按当前实际 HEAD 与这些精确 hash 继续验证；运行前后外部 HEAD/status/source/lock/binary 绑定完全一致，见 `external-head-difference.json` 与 `external-independent-before.json` / `external-independent-after.json`。

实际执行 binary SHA256 为 `2afa1e2e33aa3e73a577439d09ae64a4d59b3028081caa3bd8086858b8a3551b`。本轮没有 fresh Rust rebuild，使用已有且已绑定的外部 interop binary；原开发 `--locked --offline` 构建证据保留于开发 archive，未冒充独立构建。本轮装配仅改开发私有 fixture 的证据输出目录，外部 AGPL 模块直接只读导入，没有复制、修改或打包源码/锁文件。

实际接口范围仍是精确 Quick `/remote/v1/ws` descriptor 与私有 loopback WebSocket 映射；没有验证 WAN 或真实 Quick Tunnel。ES384/JWK 与业务 AccountVerifier 是本地 fixture，未声称真实 Logto 云登录。手机浏览器、长跑及整体验收仍由其原计划承担；本轮没有生产、手机、云用户、账号、push、部署或新 worktree 操作。

**合入门槛：本冻结 r2 PASS，原 R1/R2 已关闭；原合同与上述范围限制保持不变。**
