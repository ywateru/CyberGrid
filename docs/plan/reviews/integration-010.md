# integration-010 公共装配独立评审

结论：**BLOCKED**。冻结 `task/integration-010` 的 `dca1fe160f6857a07566a1ceb9ee91f55c3c344a` 存在两项 P2 blocking。此结论仅覆盖本次公共模块装配，不表示 integration-010 的真实云登录、外部协议、手机或长跑验收完成。

## 冻结与证据

评审以当前根合同 `docs/common-sample.md`、`docs/browser-bridge.md` 及 integration-006/007/011 的既有关闭、授权与清理约束为准。冻结 parent 为 `76fc9ee`，任务 base 为 `e4e4c1b`；24 个变更路径全部位于 `examples/ram-only-workspace/`。开始及结束时 HEAD 相同、工作树 clean；24 路径内容与开发 source manifest 全部相符。开发 manifest 列出的 12 份证据文件 hash 均核对一致，后补 INDEX 的性质及原失败记录已核对。

私有证据标签：`integration-010-independent`。其中 `freeze-before.json`、`freeze-after.json`、`commands.json`、`counterexample-command.json`、`overlap-command.json`、`SHA256SUMS` 绑定真实路径、源码、原始 harness、日志与 actual exit。公共报告不包含本机绝对路径、凭据或私有原始 hash。

## I010-P2-01：回声读取超限后没有关闭受控流

- Severity：P2；blocking：是。
- Code：`examples/ram-only-workspace/web/src/index.ts:60`，以及继续使用该 `stream` 的第 61 行。
- Contract：`docs/common-sample.md:115` 要求有界 UTF-8 echo，输出队列溢出关闭通道；`docs/browser-bridge.md` 要求宿主执行业务大小边界并保有关闭权。

读取循环在 `bytes.length > 4096` 或 UTF-8 解码失败时抛错，内层 catch 仅更新错误文本，没有清除当前 stream，也没有调用并持有 `s.close()` 的清理结果。原流因此继续占用 SDK stream 槽位并保留写入能力，后续 UI 操作仍可引用它。显示“回声流已结束或返回超限内容”不能证明流已经结束。

独立原始反例 `ui-stream-overflow.test.ts` 通过实际挂载的公共 UI 发起同步 popup，执行官方 RAM Logto 客户端的 PKCE/ES384 回调，使用真实 Node handler、Redis 和 JS SDK 完整目录/受控 wrapper。受控业务 adapter 按合法 `maxFrameBytes=65535` 返回一个 4097 字节块，故该块允许经过 SDK，随后触发样例自身的 4096 字节业务限制。没有修改产品源码、直接 seed SDK 状态或用 SDK 替身绕过公开链。

实际结果：UI 显示超限错误，readable cancel、writable abort、peer close 均为 0；业务状态仍显示已连接。断言要求样例关闭受控流，实际 `0 !== 1`，最终原样 harness 进程 **exit 1**。修复应在异常/超限路径立即撤销该 stream 的 UI 使用权，保留并驱动实际关闭；旧读取任务不得清除后来创建的新 stream。

## I010-P2-02：等待旧 close 的连接尝试可覆盖较新成功连接并遗失 peer

- Severity：P2；blocking：是。
- Code：`examples/ram-only-workspace/web/src/index.ts:23` 的 `choose()`，尤其 `await closeBusiness()` 后仅检查 account generation，以及第 38 行直接替换 `bridge`。
- Contract：`docs/common-sample.md:111`、第 134–135 行的连接取消和旧代不复活要求，以及 `docs/browser-bridge.md` 的受控 wrapper、取消迟到关闭和 owned cleanup 约束。每个已经创建的 peer 必须始终有可达的取消/关闭 owner。

`choose()` 只记录账号 generation，没有独立的连接 attempt fence。A 在旧 bridge 的慢 `close()` 上等待期间，`closeBusiness()` 已把 `bridge` 清空；B 随后进入，可以直接建立新连接。旧 close 结束后 A 仍沿同一账号 generation 继续建立连接，并直接覆盖 B 的成功结果。B 的 wrapper 从 UI 中丢失，既没有被替换时关闭，也不再是当前连接等待控制器所对应的对象。

独立原始反例 `ui-overlap.test.ts` 同样通过实际 UI 登录、官方 PKCE/JWK、真实 Node/Redis 目录及实际 JS wrapper，仅在受控 adapter 的旧 peer close 上设置闩锁：

1. 建立 P0；点击 A，让其等待 P0 close。
2. 再次点击 B，建立 P1；通过 UI 读取信息，确认 P1 已完成业务请求。
3. 释放 P0 close；A 恢复，建立 P2；通过 UI 读取信息，确认当前操作已转向 P2。
4. P1 的 close 仍为 0，连接上下文信号未取消；P1 已被替换却没有得到关闭。

最终原始 harness 断言 P1 必须得到关闭，实际 `0 !== 1`，进程 **exit 1**。测试还记录了 UI cancel 后状态，但不把“已连接后 cancel 不关闭连接”另立 finding，因为该按钮文案承诺的是取消连接等待。修复需要在所有 await 前后校验连接 attempt 身份，阻止旧 attempt 覆盖较新结果，并对每个迟到或被替换的成功结果保持关闭 owner；unknown、前台恢复及退出应同步使旧 attempt 失效。

## 独立实际验证结果

以下数量均为本轮独立执行，未借用开发者通过数：

| 验证 | Actual result |
| --- | --- |
| Sample Rust workspace，`--locked --offline --workspace -- --include-ignored`，真实 Redis required | exit 0；22 passed，0 failed，0 ignored（desktop 19、gateway-upstream 1、local-workspace 2） |
| Desktop official RAM owner suite | exit 0；5 passed |
| Web official RAM owner suite | exit 0；5 passed |
| Web actual Node/Redis/ES384 + official owner + JS directory/wrapper integration | exit 0；1 passed |
| 独立实际 UI stream overflow 反例 | exit 1；1 failed，断言关闭计数为 0 |
| 独立实际 UI overlapping choice 反例 | exit 1；1 failed，断言被替换 peer 关闭计数为 0 |
| 冻结 diff whitespace check | exit 0 |

Rust 全套实际覆盖 held bind/state 时本机 TCP/bearer 封闭、取消 public waiter 后 owned cleanup、endpoint 已观察 revision 回退、迟到 ACK 单调期限拒绝、真实默认 60 秒续期，以及真实 Native mapped TCP 的同步封门/EOF与本地服务继续。上述是本轮实际运行已有产品测试，不伪称为另写的独立反例。官方 Web owner 测试覆盖错 origin/source/state、同步 popup、回调清址、刷新单 owner、未知轮换停止重放和取消后保留 rotation。

原开发 `18/19` 失败日志保持原样，其旧断言在释放临时端口后仅以连接成功推断原 gate 未关闭，可能把并行 fixture 复用端口误认为旧 gate。冻结测试仍持有 gate 自己的 listener，先验证拒绝新业务，再验证既有/在途 socket EOF、owner join 及原本地 target 可用；本轮完整执行通过。未删除该负控，也未把原失败日志改标 PASS。

独立回声 harness 的首次有效运行已产生同一产品断言失败，但 Node 测试 runner 存在剩余句柄；原日志保留，显式终止不记作产品 exit 1。随后使用 Node `--test-force-exit` 原样运行同一 harness，取得实际 exit 1。依赖解析启动失败也单独保留，不算产品 finding。Overlap 初稿的 premature info 与错误等待“已连接后 cancel 必须 close”导致的 timeout 同样单独保留、明确不算产品 finding；最终 harness 等待 UI 确认连接后验证真正的替换所有权问题，其字节与 exit 已锁定。

## 已审边界与保留范围

源审覆盖本次 24 路径的公共装配变更：唯一 `run_with_factory` Tauri 链、默认无 factory/adapter 的 failclosed 文案、固定产品与实际 lease 能力/身份、bind 前 endpoint 订阅及贯穿监视、incarnation/addressRevision 历史、服务发布确认和单调 hard deadline、NativeClose 首 poll 的锁边界/安全 waker/单 receipt，以及 Web RAM owner、callback、身份确认和业务 DTO。Web lock 未新增 AnyWork/clatter 包或本机/git 源依赖；没有将私有外部传输代码带入公共树。

本轮没有重复执行开发者 clippy、独立 locked build 或双 Vite build；其绑定日志已核，Rust 实际测试同时完成必要编译。已知 blocking 后未继续扩展无关套件。修复后的独立 r2 必须原样执行两份最终 UI harness，并验证受影响的取消、unknown/前台恢复、stream 多字节和重叠清理边界。

受控 OAuth/JWK、DOM 和业务 adapter 是明确的 fixture；真实的是官方客户端、服务/Redis、SDK 与公共挂载代码边界。本报告不声称真实 Logto Cloud、外部 Noise、公共 Cloudflare/WAN、手机、30 分钟/8 小时/24 小时验收或部署通过。公共入口默认不携带 Gateway factory 或 Web business adapter，不能独立完成外部业务连接。integration-010 整体保持未完成。
