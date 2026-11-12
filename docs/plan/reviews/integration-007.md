# integration-007 独立评审

结论：**BLOCKED**。冻结实现存在两项 P1 blocking；构建、针对性边界、真实 Redis/ESM 与真实浏览器 Noise 通过，不能抵消这两项合同缺口。

## 评审绑定与范围

- 冻结工作树：`task/integration-007`，分支 `task/integration-007`。
- commit：`b5acbec4586beb7271616ab150bfcf084bd4e447`；base：`eae29328995696fb741b18f4ab993d3048623fec`。
- 评审使用根目录当前 `docs/INDEX.md`、task、`browser-bridge.md`、`integration-contract.md`、`metadata-and-compatibility.md`，按 docs-sprint 验证角色执行；没有仅采用冻结树旧文档。合同文件 hash 见私有 `root-contract-binding.json`。
- 全部八个变更路径均已源码审查：`sdk-js/README.md`、`sdk-js/package.json`、`sdk-js/src/index.ts`、`sdk-js/src/bridge.ts`、`sdk-js/src/directory.ts`、`sdk-js/src/endpoints.ts`、`sdk-js/test/bridge.test.mjs`、`sdk-js/test/bridge.integration.mjs`。
- 八路径 SHA256 与开发 `source-final.json` 一致；独立构建后的八项 dist hash 也与冻结清单一致。开发 `SHA256SUMS` 全部条目实算匹配，开发日志 hash 已核对；这些是开发证据审计，不计作独立测试数量。
- 本 reviewer 未修改实现。仅暂时 symlink 根目录已有 `sdk-js/node_modules` 和 `server/node_modules`，已全部移除；结束时冻结工作树仍 clean、HEAD 未变。
- 独立证据目录：`private-evidence:integration-007-independent`。所有实际命令、cwd、exit、耗时见 `commands.json`；源码与证据 hash 见 `freeze-binding.json`、`freeze-after.json`、`SHA256SUMS`。

## Findings

### R1 — P1 / blocking：可信配置移除已选产品后，目录和新桥接仍被接受

合同：根目录 `docs/integration-contract.md:26` 要求重新读取完整可信配置时保持已选 product，原 product 不再被允许必须明确失败并封闭远程访问；`docs/browser-bridge.md:71` 要求策略改变沿完整可信配置链验证后再接受地址。

代码：冻结 `sdk-js/src/index.ts:138` 的 `config()` 在 141–144 行只检查 `products` 的字段形状和重复，随后丢弃产品集合；287–295 行 reload 只确认 epoch、endpointPolicy 存在和 hash 相同，未检查本次选择的 product 是否仍被当前 client 允许。`directory()` / `connectBridge()` 随后可继续使用该产品。

独立反例：先取得包含 `Core` 的完整可信配置并建立 `Core` 通道；随后目录宣告新的 endpointPolicyVersion，可信配置返回新的产品 policyVersion、`products=[Other]`，其他 OAuth 绑定保持一致。SDK 接受仍由旧实例返回的 `Core` 完整目录，并能使用独立 adapter 再建立 `Core` 通道。最终日志记录 `OBSERVED removed product admitted Core replacement gate connected`，要求目录明确拒绝的断言失败。

这不是要求 SDK 提供跨所有旧部署的即时全局撤销；是 SDK 已经明确读到当前可信配置移除 `Core` 后，仍继续接受它。合同明确允许旧配置实例短暂存在，恰好使本反例具有实际意义。

证据：`counterexamples.test.mjs` 第一项，`counterexamples-final.log`，**actual exit 1**。建议修复时保留经过验证的允许产品集合，并让所选产品校验同时覆盖 reload、后续目录和新连接；拒绝时保持 gate 关闭，不迁移到另一个产品。

### R2 — P1 / blocking：完整目录发出 unknown 后，现有业务 gate 仍开放

合同：根目录 `docs/browser-bridge.md:69` 要求目录失效/不可达输出 unknown，不能继续声明 ready；77、81 行要求业务 gate 随目录失效先关闭，流和在途请求受关闭信号约束。此要求适用于不能信任完整目录的失败，不能仅覆盖网络错误。

代码：冻结 `sdk-js/src/index.ts:250` 的 `observeDirectory()` 在 253 行只把 `NETWORK_ERROR`、`TIMEOUT`、`SESSION_UNAVAILABLE` 转为 `coordination=unavailable`，从而触发 118 行全局 invalidators；`INVALID_RESPONSE`、`FORBIDDEN`、`DIRECTORY_TOO_LARGE` 等仍保留 connected。304–305 行的其他 watch 可以先发出 unknown，而原桥接只等自己的下一次 watch 才关闭。

独立反例：已连接桥接拥有自己的串行 watch；新增独立 watch 立即取得含禁止 `iroh` 字段的畸形 console 完整目录，解析正确拒绝并发出 `unknown / INVALID_RESPONSE`。在该 unknown 到达后，旧 wrapper 仍是 `phase=connected`，实际 `request('write-after-unknown')` 已派发一次并返回相同业务结果。最终日志明确记录 gate 状态、业务 reply 与 adapter dispatch count 1；要求 gate 已关闭的断言失败。

这里没有用裸 adapter 绕过 wrapper，也没有等轮询窗口后检查；失败经过真实 SDK parser、watch 事件与公开 wrapper request，说明 unknown 与业务 gate 的关闭次序不满足合同。

证据：`counterexamples.test.mjs` 第二项，`counterexamples-final.log`，**actual exit 1**。建议让有效完整目录的获取/校验失败在对外状态通知和 unknown 之前同步封闭相应业务 gate，涵盖解析、权限与容量错误；保留取消/用户输入错误的合理边界，避免只按当前三个 code 筛选。

## 独立验证结果与限制

| 验证 | 实际结果 | 证据 |
| --- | --- | --- |
| TypeScript build | exit 0，dist 全部 hash 匹配冻结清单 | `build.log`、`package-independent.json` |
| 两项正式 blocking 反例 | 0 pass / 2 fail，exit 1；失败为合同断言 | `counterexamples-final.log` |
| 既有边界用例的独立针对性执行 | 11 pass / 0 fail / 0 skip，exit 0 | `targeted-boundaries.log` |
| 新写的独立边界用例 | 6 pass，另新增恢复预算 1 pass；分别 exit 0 | `acceptance.log`、`recovery-budget.log` |
| 构建后 ESM → 两 HTTP handler → ES384/JWK → 真 Redis | 1 pass / 0 fail / 0 skip，exit 0 | `redis-http.log` |
| 实际浏览器 clatterjs ↔ Rust snow Noise IK | 14 checks，exit 0，140000-byte echo hash 匹配 | `browser-noise.log`、`noise-browser-result.json` |
| 包 dry run / 依赖许可边界 / diff whitespace | exit 0；依赖字段未变，无外部源码打包 | `pack-dry.log`、`package-independent.json`、`diff-check.log` |

独立通过的单元/边界用例共 **18 个 distinct**（11 个针对性既有用例 + 7 个新用例）；没有声称独立重跑整套 51。开发证据是完整 49 + 额外 2 distinct，另有重叠 5 项；这些数字不进入独立计数。Redis 的 1 个集成用例与 Noise 的 14 个 check 单独列出，不与单元数量相加。legacy HTTP 本轮审计了开发日志与 hash，未重复执行。

源码审查与针对性反例覆盖：完整 console 投影的 product/epoch/iroh/重复项/bridge 期限拒绝；provider 规范语法和策略 hash；串行 watch、取消后的迟到响应；较旧 in-flight watch 不覆盖握手后的较新确认；无 raw peer/stream 逃逸；每次 request/open/read/write gate；未 settled 请求/流保留容量；帧限制、复制与单次读写背压；取消后的迟到 peer/stream 归属；close 最多五秒且 cleanupPending 保留；单调独立到期、wall-clock 回拨、changed=false 且更高 publishRevision 的续期；请求起点保守期限；总连接预算同时包含目录、实际 session recovery/config fetch 和握手。默认 60 秒及 1–120 秒范围经源码确认，实际短预算反例为 1000ms，没有浪费一分钟做默认值等待测试。

2 MiB 的排除项：初始诊断曾要求畸形 HTTP 200 的超大原始 body 本地越界也返回 `DIRECTORY_TOO_LARGE`，该断言失败，保存在 `counterexamples.log` / `counterexamples-initial.test.mjs`。准确合同规定服务端目录超限返回固定码，未额外规定本地 bounded reader 越界固定码，因此**不列 finding**。后续实际确认 HTTP 200 超 2 MiB 被 `INVALID_RESPONSE` 拒绝，服务端 HTTP 413 `DIRECTORY_TOO_LARGE` 被保留（`acceptance.log`）。容量/解析失败仍属于 R2 的 gate 问题，不能因错误码讨论而忽略。

独立 Noise 使用开发私有装配的复执行，改写证据输出目录，导入未修改的外部实现。外部 AnyWork commit `4f23c452b61c935720980cf715fa483844c0b507`；source/lock/完整 remote crate source hash 与已记录绑定一致，binary SHA256 `2afa1e2e33aa3e73a577439d09ae64a4d59b3028081caa3bd8086858b8a3551b`。本轮前后 HEAD/status/source/lock/binary 均一致（`external-independent-before.json` / `external-independent-after.json`）。开发已记录 `cargo build --locked --offline` exit 0；本轮直接运行该已绑定 binary，没有将开发构建冒充独立重建。

真实浏览器链实际通过错误 key、错误 installation、短授权 ACK 续期、续期拒绝并中止在途业务、真实流读写、140000-byte 分块密文请求/echo、目录撤回、取消后迟到认证 peer 关闭与 wire 中不出现业务 token 等检查。echo SHA256 为 `717721f9f1f029e636862a903c88a00ea1cdd5c0d30942eb79533f44a7a1885e`。公开 Redis 测试的 adapter 是生命周期替身，密码学证据来自本条真实外部链，未将 `encrypted=true` mock 当 E2E。

外部接口仅验证精确 Quick endpoint `/remote/v1/ws`，私有 WebSocket 映射到 loopback；不是 WAN/真实 Quick Tunnel。业务授权使用 fixture AccountVerifier，本轮 JWK 是本地签名 fixture，均非真实 Logto 云登录。没有手机浏览器、生产、账号、云用户操作、push、部署或新 worktree；没有复制/修改/打包 AGPL 源码及锁文件。

## 交付门槛

**BLOCKED：R1、R2 均需开发 owner 修复后，按新冻结 commit 重新验证这两项失败反例与受影响边界。** 本 reviewer 没有发现需要改变合同来容纳实现的理由，也没有将上述环境限制伪称为已通过的交付能力。
