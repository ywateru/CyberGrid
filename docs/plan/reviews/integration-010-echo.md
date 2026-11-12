# integration-010 echo 独立评审：BLOCKED

独立 verify 绑定 `task/integration-010-echo` 冻结 `fabaccd3363379e33c30dccadcfcc588d0402d53`，base `939d862e74fe9298612ca90d4dfdede71b5d9092`。仅 `examples/ram-only-workspace/gateway-upstream/src/lib.rs`、`src/tests.rs` 两路径变更，开始及结束 clean。评审未修改实现、外部 adapter 或私有传输。

正常输入的 JSON 封套误回传问题已修复，独立六组实际 WS 边界全部通过；但严格输入类型仍存在一项 **P2 blocking**，本冻结不可合入。

## I010E-P2-01：枚举反序列化额外接受对象形态的 type

- **Severity：P2；blocking：是。**
- **合同：** `docs/common-sample.md:117` 要求 Gateway-only terminal 在既有 bearer 首帧认证后，仅接受精确文本 JSON 对象 `{"type":"input","data":"合成文本"}`；非法输入应关闭远程流。
- **代码：** `examples/ram-only-workspace/gateway-upstream/src/lib.rs:139` 定义的 `TerminalInputKind` 使用 Serde 默认外部标记枚举表示，`TerminalInput` 在第 148 行将其作为 `type` 字段。第 181 行解析因此同时接受字符串 `"input"` 和对象 `{"input":null}`；`deny_unknown_fields` 不会限制这个枚举字段的 JSON 形态。
- **最小实际重现：** 调用公开 `GatewayUpstream::start(Workspace::new())`，连接实际 loopback `/ws/terminal/independent`，先发送正确的 `{"token":local_token}` 首帧，再发送文本 `{"type":{"input":null},"data":"must-not-echo"}`。预期一秒内关闭且无业务输出；实际收到 **Binary `must-not-echo`**。
- **证据：** 私有标签 `integration-010-echo-independent` 中 `frozen-harness/tests/type_shape.rs` 是独立最小反例；`frozen-strict-type-counterexample.log` 与 `frozen-after-commands.json` 记录实际 Cargo **exit 101**。依赖使用冻结树的明确源码路径，日志确认该源码实际编译，非 encrypted/mock 或内部状态注入。
- **修复要求：** `type` 必须验证为精确字符串 `input`，拒绝对象、数组、数字、布尔及 null；保持未知/重复字段拒绝、4096 帧/消息及 data 字节上限、有界发送/关闭与独立连接隔离。原反例字节及失败日志保留，复审原样重跑，并增加各错误类型形态的负控。

本问题是在已认证 Gateway-only 合成协议中的形态校验缺口，不是鉴权绕过、任意命令执行或完整 PTY 能力。P2 blocking 来自明确的精确协议合同。

## 已独立运行的实际验证

独立 Rust harness 通过公开 API 启动真实 TCP/HTTP/WebSocket 服务；测试源码在 before/after 保持相同字节，依赖绑定分别记录。没有修改产品源码来制造 before 状态。

| 场景 | 实际结果 |
| --- | --- |
| 旧实现 before | 对 root 中逐哈希等于 `939d862` 的相关源码运行锁定正例，实际 exit 101：收到整个 `{"data":"first","type":"input"}`，而非 Binary `first`。 |
| 合法输入及输出语义 | 冻结实现通过多次输入、中文/emoji、转义、显式 Unicode surrogate pair、空串、NUL，以及内容本身长得像 JSON 的 data；只返回 data 原始 UTF-8 Binary，保持原始输出语义。 |
| 非法业务输入及隔离 | 非 JSON、null/数组/数字、未知操作、缺字段、错误 data 类型、额外/重复字段、非法 surrogate 和二进制业务输入均关闭；其他已鉴权流继续可用。 |
| 大小与 frame | 精确 4096 字节 wire 正例及多字节边界通过，4097 字节拒绝；UTF-8 字符跨真实分片帧正确重组，分片累计超限及非法 UTF-8 文本关闭。 |
| HTTP/首帧认证边界 | 无 bearer HTTP 拒绝、正确 bearer 成功；WS Origin/cookie/HTTP authorization 拒绝；错误 token、业务帧或二进制冒充首帧认证均关闭。 |
| 远程关闭与本地继续 | remote seal 关闭实际远程 WS 并拒绝新连接；同一已有本地浏览器 WS 在正确 cookie/CSRF 认证后继续回声。 |
| 六组完整独立 after | `frozen-harness/tests/boundary.rs` **6 PASS / 0 fail / 0 ignored，exit 0**。新增严格 type 反例单独 **exit 101**，不能被这六组通过抵消。 |

作者冻结的五项测试及同 harness before/after 证据已核源码与哈希绑定，不计为上述独立六项。作者 strict Clippy 是开发证据，本轮未重复其他模块、全 Rust/Web、云或手机验证。

## 证据与验证边界

首次独立 after 尝试仅切换私有 dependency symlink，Cargo 因 freshness 缓存复用了旧实现。该次日志已单列 reviewer cache diagnostic，未作为冻结实现的行为证据。随后改用独立 manifest 中明确的冻结源码路径，Rust 测试字节未变，日志确认重新编译实际冻结 Gateway，才得到六组通过和严格 type 反例失败。未 clean 共享 target，也未触碰产品源码以强制重编译。

私有证据标签 `integration-010-echo-independent` 保存源码/lock/harness/可执行产物绑定、实际命令 env/exit、before 原始失败、最终六项通过、最小新反例及 `SHA256SUMS`。共享 target 使用既有目录，debug info/incremental 均关闭，无新增 target/toolchain。临时私有依赖链接已移除，无运行中的 Cargo。

本轮只验证公共合成 Gateway-only terminal 子修复；没有复测真实公网 ExternalGateway 链，不把 loopback 受控测试计为 WAN、Noise、Logto、手机或长跑验收。没有复制外部 AGPL 实现，没有云/生产、push/tag/Actions/deploy 操作。报告生成后的 exact source-policy 纳入由 root 另行处理。
