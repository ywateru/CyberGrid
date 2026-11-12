# integration-010 公共装配独立复审 r2

结论：**PASS（本次公共装配修复范围）**。冻结 `task/integration-010` 的 `141dcd17ad66ef4f1e392be1e32668c321ccaf54` 已解决前轮 I010-P2-01 和 I010-P2-02，本轮未发现新的 blocking。原 `integration-010.md` 的 BLOCKED 结论继续保留，绑定其旧冻结，不改写为通过。integration-010 整体运行、发布和长期验收仍未完成。

## 冻结与审查范围

Parent 为 `dca1fe160f6857a07566a1ceb9ee91f55c3c344a`。本轮 delta 精确为五个 sample 路径：README、Web package、`web/src/index.ts`、`web/test/ui-fixture.ts` 和 `web/test/ui.integration.ts`，均已审查；累计 26 个任务路径的内容与开发 r2 manifest 全部吻合，21 个继承路径与前轮内容相同。Rust、Native close、Host publisher、desktop owner、授权实现及锁文件均未变化。开始和结束 HEAD 相同，工作树 clean，临时依赖链接已移除。

本轮按当前根 `docs/common-sample.md`、`docs/browser-bridge.md` 和任务 010 的执行证据规则评审。私有证据标签为 `integration-010-independent-r2`：`freeze-before.json`、`freeze-after.json`、`commands.json`、`report-binding.json` 和 `SHA256SUMS` 保存精确路径、源码/合同/原始 harness 绑定及 actual exit。开发 r2 manifest 的 17 份证据文件 hash 均已核对；开发通过数没有计入独立结果。

## 原问题复验与关闭结论

### I010-P2-01：已解决

`examples/ram-only-workspace/web/src/index.ts:79` 的流失败处理先撤销当前 UI 流/连接使用权，再持有并驱动受控 stream 和 parent 的 close。`closeStream` 在第 21 行开始保存实际关闭 Promise；一秒后仍未确认时封闭 parent，其异步清理继续保留 owner。`streamRevision`、parent 和当前 stream 三重校验隔离旧读写任务，不允许迟到错误更新新连接。

原独立 `ui-stream-overflow.test.ts` 未修改字节，实际重跑 **exit 0，1 PASS**。相同实际 UI/官方 RAM PKCE/Node+Redis/JWK/JS wrapper 链再次接收合法 SDK frame 范围内的 4097 字节业务块；read cancel、write abort、peer close 由旧冻结的各 0 变为各 1，UI 显示业务已关闭。

独立新终态测试还验证了 split UTF-8（含非 BMP 字符）、EOF、非法 UTF-8、overflow、write failure 后无法继续从 UI 写入，以及未完成 stream close 时约 **1005 ms** 封闭 parent、清理迟到结束后不再创建替代 stream。终端 SDK read rejection 采取保守关闭 parent 的行为与 README 一致；没有宣称所有 EOF 都保持 parent 可用。

### I010-P2-02：已解决；原 overlap harness 本轮仍为失败，不能记作 PASS

`examples/ram-only-workspace/web/src/index.ts:43` 在等待旧 close 前创建独立 attempt，其单调 revision、账号 generation、对象身份和取消信号共同决定有效性。新选择、cancel、unknown、前台重查和 logout 同步使旧 attempt 失效；第 44、49、56 行分别阻止旧等待继续派发、旧确认后继续连接及迟到结果提交。已创建的结果保留实际关闭 owner。

原独立 `ui-overlap.test.ts` 保持原字节，实际重跑结果为 **exit 1，timeout**，停在第 73 行的 `peers.length === 3`。该前置强制要求旧 A 在 P0 close 结束后继续派发 P2，才能走到最后的泄漏断言；正确 fencing 应阻止这次派发。因此它在新冻结上不能表达正确终态，**本报告没有把该 timeout 作为通过**。旧冻结上的实际泄漏断言 exit 1、原 harness 和本轮 timeout 都完整保留。此处理遵循当前 task010 的原始反例前置不适用规则，没有为兼容旧 harness 放宽产品合同。

另由独立 reviewer 编写 `terminal-state.test.ts` 及自己的 `real-ui-fixture.ts`，使用公开 UI 登录和实际服务/SDK链验证正确终态：P0 close 被扣住时 B 成功并处理请求，释放 P0 后 A 不派发，peer 数保持 2，B 继续处理请求；另一个已派发但迟到的 A 在 B 成功后必须实际关闭，B 不受影响。没有 seed SDK 内部状态，也没有替换 SDK 方法。

## 本轮独立 actual results

| 验证 | 实际结果 |
| --- | --- |
| 原 overflow harness，字节不变 | exit 0；1 PASS；cancel/abort/peer close 各 1 |
| 原 overlap harness，字节不变 | exit 1；第 73 行等待被 fencing 禁止的 P2 超时；不计 PASS |
| 独立新实际 UI 终态 suite | 自然 exit 0；9 PASS、0 FAIL、0 skipped；未使用 force-exit |
| 冻结公共 `test:integration` | 自然 exit 0；8 PASS、0 FAIL、0 skipped；未使用 force-exit |
| Web official owner suite | exit 0；5 PASS |
| Web typecheck + Vite build | exit 0 |
| 冻结 delta whitespace check | exit 0 |

独立新 9 项分别覆盖：B 持续 current 且旧 A 不派发；已派发迟到 A 的 owned close；held pre-dispatch A 被 cancel、真实 HTTP 503 watch unknown、foreground、logout 分别封闭；EOF/split UTF-8/invalid/overflow/write failure 清理；旧读写迟到结果隔离；一秒未完成 close budget 封 parent。真实 watch unknown 经实际 HTTP 错误进入 SDK 及公共 UI，未直接调用内部状态回调。所有 suite 数量分列，不把继承、重复执行或开发者数量相加为新覆盖数。

## 清理 owner 与页面引用

`examples/ram-only-workspace/web/src/index.ts:8` 在 mount 时捕获所属 page window/document，并用同一对象创建节点、确认身份、注册和移除生命周期 listener。该引用绑定实际页面 owner，避免之后清理错误引用其他全局对象；没有增加持久化或旁路窗口。重复 dispose 仍返回同一个清理 Promise，`end().finally(...)` 保留真实失败传播，并未吞掉 after-hook failure。原 overflow harness 的清理顺序会先恢复测试全局，再 dispose；本轮原样执行已实际通过，独立自然退出 suite 也无清理异常。

Stream/parent close Promise 由 cleanup 集合持续持有，底层 SDK 保持同步 gate 与有界 parent cleanup。旧 read/write 结果和取消后的连接结果在提交前核对当前 owner，不恢复可用性、不重放写入。README 说明了这些边界，package 的 integration glob 实际纳入新 UI suite；没有新增外部源码/依赖或扩大协议支持。

## 继承与限制

本轮未重复 Rust 22 项、真实 61 秒续期、Native TCP close、desktop owner、clippy 或 native build；相关源码/锁 hash 与前轮一致，前轮独立 actual exit 证据按精确绑定继承，不声称这些在 r2 新运行。

官方客户端、Node handler、Redis、JWK 验证、JS SDK 和公共挂载代码是真实边界；DOM、OAuth 身份及 adapter 传输是明确的受控 fixture。此 PASS 不证明真实 Logto Cloud、外部 Noise/Cloudflare、手机/WAN、30 分钟/8 小时/24 小时验收或部署完成。默认公共入口仍没有 Gateway factory / Web business adapter，远程业务保持不可用；最终 integration-010 的运行和发布门槛继续由任务及 root 调度持有。
