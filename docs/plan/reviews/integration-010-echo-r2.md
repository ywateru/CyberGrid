# integration-010 echo r2 独立评审：PASS

独立 verify 绑定 `task/integration-010-echo` 冻结 `8d12451ea96854f696c7cf398bd5d43d0e6993bb`，parent `fabaccd3363379e33c30dccadcfcc588d0402d53`。仅 `examples/ram-only-workspace/gateway-upstream/src/lib.rs`、`src/tests.rs` 两路径变更，开始及结束 clean，源码哈希匹配开发 manifest。评审未修改实现，未添加依赖链接。

**I010E-P2-01 已关闭，无新增 blocking。** 原 `integration-010-echo.md` 的 BLOCKED 报告和失败证据保留。本 PASS 仅覆盖公共 Gateway-only 合成 terminal 子修复，不替代 integration-010 整体或真实公网链验收。

## 修复审查与原反例

原实现将 `type` 反序列化为 Serde 外部标记枚举，额外接受 `{"input":null}` 对象。r2 将该字段改为严格 `String`，并在生成任何业务输出前检查其值恰为 `input`。`deny_unknown_fields`、重复字段拒绝、4096 字节 frame/message 上限、data UTF-8 字节上限、发送/关闭预算、bearer 首帧认证与 stop 优先处理保持。

独立在原路径原样执行锁定的 `frozen-harness/tests/type_shape.rs`，harness 字节哈希与 r1 锁定值一致：正确 bearer 首帧后发送 `{"type":{"input":null},"data":"must-not-echo"}`，修后无业务输出并关闭，**1 PASS，实际 exit 0**。原冻结 `fabaccd3` 的实际 exit 101 日志、源码及可执行产物保持原字节，没有重写成成功证据。

## 独立实际验证

所有测试均通过公开 API 启动真实 loopback TCP/HTTP/WebSocket 服务，没有 mock 或内部状态注入。

| 验证 | 结果 |
| --- | --- |
| 原最小反例 | 原样重跑，1 PASS，exit 0。 |
| 原独立六组 | Rust 测试字节与 r1 相同；6 PASS / 0 fail / 0 ignored，exit 0。 |
| 新独立 type matrix | 1 PASS，exit 0；包含下述 20 个非法形态/值和两个正例。 |

类型矩阵覆盖原枚举对象、空对象、对象内错误值/大小写、数组、整数/浮点数、bool、null，以及空串、大小写差异、前后空格、换行、NUL、相似 Unicode 字符等字符串。每次使用独立已认证流验证一秒内终止、无业务输出；同一 survivor 连接在每次拒绝后继续正常回声。精确 `"input"` 和解码后同值的 `"in\u0070ut"` 均按正常 JSON 字符串语义接受，不做大小写或空白归一化。

原六组再次覆盖：

- 多次输入、中文/emoji、显式 Unicode 转义、换行/引号/反斜杠、空串和 NUL；data 本身像 JSON 时仍作为原始 UTF-8 Binary 输出，不解包正常业务输出。
- 非 JSON、错误顶层形态、未知操作、缺字段、错误 data 类型、额外/重复字段、非法 surrogate 及二进制业务输入拒绝，独立已认证流继续可用。
- 精确 4096 字节 wire 正例、多字节边界、4097 字节拒绝；真实分片 UTF-8 重组、累计超限和非法 UTF-8 文本关闭。
- HTTP bearer、WS Origin/cookie/HTTP authorization 与首帧 token 隔离；remote seal 关闭真实远程流并拒绝新连接，同一本地浏览器 cookie/CSRF 会话及 WS 继续可用。

一秒关闭断言验证了实际正常收发条件下的终止；源码仍对发送和最终 close 设置独立一秒预算。本轮没有宣称测得持续网络背压下的全部调度时延。

开发五项 affected suite 和 strict Clippy 仅作为已核冻结绑定的开发证据，未计为本轮独立数量。独立新增运行总计为 **1 + 6 + 1 个测试**，没有重复其他模块、全 Rust/Web、云或手机套件。

## 证据、收尾与限制

私有证据标签 `integration-010-echo-independent-r2` 保存源码/manifest/lock/harness/可执行产物绑定、原 r1 失败证据未变证明、实际命令 env/exit、三个通过日志及 `SHA256SUMS`。全部 Cargo 使用既有共享 target、installed stable、locked/offline，debug info/incremental 关闭；无 clean、新 target 或 toolchain 下载。运行结束后已释放共享 target，无 live exec。

原 BLOCKED 与本次 PASS 报告生成后的 exact source-policy 纳入、实际文档字节及导出检查，按 root 安排另做小冻结和私有 closeout，不创建公开 r3。当前代码评审通过不表示该后续冻结已完成。

本轮没有修改外部 adapter、私有传输或输出规范化，没有复制外部 AGPL 实现，没有公网 ExternalGateway 链、真实 Noise/Logto、云、手机、自然长跑、生产、push/tag/Actions/deploy 操作；这些均不计为本次 PASS。
