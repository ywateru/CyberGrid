# integration-006 r2 独立复审

结论：**PASS**。冻结 commit `d952d25bbf5900415fa7bbf6b1073fe0c26c3474` 关闭原 R1 的 P1 blocking；本轮没有发现新的 blocking 或 non-blocking finding。结论适用于下述修复、冻结绑定及实际验证范围，由 root 决定合入。

## 冻结绑定与范围

- 分支 `task/integration-006`；parent `8ae09996feb5c0d961630fdccf85d4d08b38703b`，原 feature base `564edb7d85a4db47de10162893a6c769e3ed1eb0`。
- 采用 root 当前 `docs/INDEX.md`、task、`registration-admission.md`、`integration-contract.md`、`browser-bridge.md`、`metadata-and-compatibility.md`，按 docs-sprint 独立验证角色执行；合同没有放宽。具体 root HEAD 与合同 hash 见私有 `root-contract-binding.json`。
- 完整审查 r2 三路径 diff：`sdk-rust/src/client.rs`、`sdk-rust/src/capability_tests.rs`、`sdk-rust/README.md`。累计 13 路径 source hash 全部匹配开发 `integration-006-development/r1/source-final.json`；其余十路径未改，原源码审查结论保留。
- 开发证据 JSON 与清单指定 hash 匹配，六份开发日志 hash 均实算匹配。开发完整 78/78、capabilities 18、clippy、example、consumer build 及原 harness 通过只计为开发证据审计，没有充作独立运行数量。
- 独立证据标签：`integration-006-r2-independent`；索引 `INDEX.md`，命令与 actual exit 见 `commands.json`，冻结绑定见 `freeze-binding.json` / `freeze-after.json`，证据校验见 `SHA256SUMS`。精确本机路径仅保存在私有 archive。
- reviewer 未修改实现、产品 Cargo manifest/lock 或 Lab。复用已有共享 Cargo target，`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_INCREMENTAL=0`，正常 Cargo 锁协调，无 clean、新 worktree 或新大 target。临时 server 依赖 symlink 已移除，最终冻结树 clean、HEAD 未变，运行进程全部结束。

## 原 R1 复验

**R1 — 原 P1 / blocking，现 CLOSED：明确 expired unknown 后，已确认 owner 的公开显式 fresh 登录入口恢复可达。**

合同：`docs/registration-admission.md:49` 区分操作明确过期与旧提交从未成功；`:53` 要求未知操作明确过期后停止后台重放，仅允许宿主显式 fresh 或完整可信新 epoch 开始新域；`:55` 明确 unknown 401 不证明过期；`:57` 要求此前已有可信 `Session.user`，首次 owner 未确认继续 Unknown/Pending。`docs/integration-contract.md:32`、`:34`、`:42` 保持唯一 owner、精确重放、固定身份与公开恢复入口。

代码审查：

- `sdk-rust/src/client.rs:1268` 的 supervisor 在 `ExchangeOutcomeUnknown` 且私有操作已经由服务明确标记 expired 时发出 requires-login 并退出重试循环，随后释放 mutation 锁。该分支不生成新域、不换 token/requestId，也没有抹掉旧 pending。
- `client.rs:676` 的公开 login guard 只为 `unknown && expired` 且已有可信 original user 的操作开放显式恢复；`:686` 在此路径禁止直接把旧 active session 当成 fresh 成功返回。
- `client.rs:872` 仍要求显式 Connect 且此前 subject 已确认；`:905` 通过唯一 AuthProvider 请求 `forceRefresh=true`，`:913` 保留旧未知关闭事实。未知 401 不满足此条件；首次未确认 owner 也无法借不透明票据取得该资格。
- `client.rs:411` 的 requires-login 状态保留 `AdmissionExpired` 和不可自动重试的分类，使宿主能区分明确过期与一般授权/协调失败。
- `client.rs:1110` 后的既有 subject 比较及精确迟到 session 撤销、`:1714` 后的本机先关闭与 Pending 清理逻辑未被放宽。

首先把原独立 `public-harness/src/main.rs` **字节不变**复制到本轮私有证据，在新冻结源码上执行 `cargo run --locked --offline`，**actual exit 0**。原公开流程仍是正常首次登录 → 可信新 epoch → 后台未知注册 → 实际 Redis TIME 明确拒绝过期 → 宿主公开 `login_with_auth_provider()`。没有修改 Client 私有字段或本地墙钟。

运行准确记录 owner 序列 `Connect,false → CoordinationRecovery,false → Connect,true`；显式 fresh 在三秒预算内完成，本机 gate 保持 false；`disable_remote()` 返回 `localClosed=true / coordinatorRevocation=Pending`。原 `8ae0999` 冻结对应的三秒超时与 exit 101 证据保留于上一轮 archive，不改写历史结果。

## 独立公开负控与实际结果

新增私有 `public-harness/src/bin/public-negatives.rs` 仅使用公开 SDK API、公开 snapshot 和真实 fixture 控制端点。五个场景均通过，组合程序 **actual exit 0**：

| 场景 | 实际断言 |
| --- | --- |
| 明确过期后 fresh 返回不同已验 subject | 公开调用返回 `IdentityChanged`；原可信 subject 留存，gate 关闭，旧清理仍 Pending；owner 本次请求为 Connect/true |
| 已知 owner 的未知注册重放得到真实 INVALID_IDENTITY 401 | 状态不冒充 AdmissionExpired；后续所有注册 body hash 与原未知操作相同；显式调用没有获得新 session，owner 仍只有首次 Connect/false 与 CoordinationRecovery/false，没有 fresh；关闭 Pending |
| 首次未知 mint 后收到明确过期 | 没有可信 session；后续多次公开 provider 登录和不同 token 登录均为 `ExchangeOutcomeUnknown`；操作调用数不增加、owner 不被再次请求，gate 关闭，关闭 Pending |
| 首次未知 register 后收到明确过期 | 与未知 mint 相同，保持未知 owner 与原操作，不因明确过期而创建新域或确认清理 |
| 已知 owner 显式 fresh 的注册已经提交、响应被延迟时关闭 | 本机先进入 Disabled；放回迟到响应后 login 返回 `Cancelled`，不交付新 session；关闭返回 localClosed/Pending；enable 和再次 login 均不能复活该 Client |

两次实际 Cargo 执行分别为原 harness 29.48 秒、新公开负控 17.12 秒（含编译），详情见 `public-original.log`、`public-negatives.log`。这些是 **1 条原公开反例流程 + 5 个新公开场景**，不称为独立完整 78 个单测。每个场景实际运行 Node、本地 ES384/JWK 校验和隔离真 Redis；fixture 建立两个 handler，本轮公开客户端固定使用 left 入口，未声称重跑原跨两 Native/handler 场景。没有以内部状态准备或 mock 替代公开恢复入口。

后台停止也由公开行为和源码共同确认：运行等到固定 `AdmissionExpired / retryable=false` 且 identity 为 RequiresLogin；此后公开 fresh 能取得 mutation 并完成。负控还检查等待期间 admission 操作调用保持稳定。unknown 401 则保留精确重放与 Pending，没有为了释放显式 fresh 而把 401 当成过期。

## Pending、锁与既有边界

`retired_unknown` 保持有界保守事实。明确过期只证明原操作不能再提交，不证明原操作从未成功；fresh 成功、不同 subject 被拒绝或迟到 session 被精确撤销，都不能单凭已知新结果清除旧未知清理事实。本轮所有相关关闭仍 Pending，未把难以清除 Pending 作为缺陷，也未要求放宽合同。

私有 harness Cargo.lock 与原独立锁逐字一致，产品 Cargo.lock 未变。实算核对所有非私有 root package 的版本、source 和 checksum 都与冻结产品锁一致，额外 package 仅为私有 harness root；两次均用 `--locked --offline`，未更新产品依赖。源码、锁、独立 main 与负控 source、实际执行 binary 的 hash 见 `harness-lock-binding.json` / `freeze-after.json`。

原评审的 13-path 源码审查与真实 provider WSS/mTLS 证据保留；本轮复核旧 `capabilities.log`、原反例日志、harness source/lock 与原校验清单均匹配。r2 没有修改 provider 语法、authority/显式端口、`/connect`、内层 mTLS、smux、iroh 或 WebRTC 实现。本轮没有重新执行这些不受修复影响的大套，也没有将旧树结果改写成新冻结 runtime。原 Lab baseline 16/17 的既知断言与 root 后续修复的区分保持原报告结论；最新 root 组合验证仍由 root 调度。

## 范围限制与交付门槛

过期由明确标注的签名 fixture claim 触发，实际经过服务验签与 Redis TIME 拒绝；没有等待自然 24 小时，也没有篡改本地墙钟。JWK、业务身份和账号都是本地 fixture，未声称真实 Logto Cloud、WAN、手机或长跑验收。没有生产、云用户、push、部署或许可边界变更。

**合入门槛：本冻结 r2 PASS，原 R1 已关闭；unknown 401、首次未知 owner 和旧关闭 Pending 的保守合同全部保留。**
