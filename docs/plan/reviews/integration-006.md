# integration-006 独立评审

结论：**BLOCKED**。一项 P1 blocking：已确认 owner 的后台准入恢复进入明确 expired unknown 后，合同允许的公开显式 fresh 登录入口不可达。已通过的内部分支测试不能替代真实公开调用链。

## 冻结绑定与审查范围

- 工作树 `task/integration-006`；commit `8ae09996feb5c0d961630fdccf85d4d08b38703b`；parent/base `564edb7d85a4db47de10162893a6c769e3ed1eb0`。
- 使用根目录当前 `docs/INDEX.md`、`docs/plan/tasks/integration-006.md`、`registration-admission.md`、`browser-bridge.md`、`integration-contract.md`、`metadata-and-compatibility.md`，按 docs-sprint 的独立验证角色执行。
- 13 个路径全部审查：`apps/tauri-test/src-tauri/src/browser.rs`；`sdk-rust/README.md`；`sdk-rust/src/{auth,capabilities,capability_tests,client,credentials,endpoints,lib,state,tunnel,types}.rs`；`sdk-rust/test-support/coordinator.ts`。
- 13 项 source SHA256 与开发 `source-final.json` 完全一致；`development-evidence.json` 与其声明 hash、五份开发日志 hash 均实算匹配。开发目录没有 `INDEX.md`，本轮据实际存在的两个 JSON 清单审计，不虚构索引内容。
- 独立证据位于 `private-evidence:integration-006-independent`；见 `INDEX.md`、`commands.json`、`freeze-binding.json`、`freeze-after.json`、`SHA256SUMS`。
- 未修改实现、产品 Cargo.lock 或 Lab。临时依赖 symlink 在结束前移除，冻结工作树仍 clean。使用 root 指定共享 target `apps/tauri-test/src-tauri/target`、`CARGO_PROFILE_DEV_DEBUG=0`、`CARGO_INCREMENTAL=0`，没有 clean 或创建另一个大 target。

## Findings

### R1 — P1 / blocking：明确 expired unknown 后，已知 owner 的公开显式 fresh 被恢复 supervisor 与 login guard 阻断

合同：根目录 `docs/registration-admission.md`「固定错误」后的续签/未知操作段要求：未知 mint/register 收到明确过期后停止旧操作重放、保留关闭待确认事实；之前已有可信 `Session.user` 时，宿主可显式重试登录，获得新鲜同身份授权后开始新域；后台不能代为续域。首次 owner 从未确认的情况继续 Unknown/Pending，二者不得混同。`docs/integration-contract.md` 第 2 节要求明确区分已知 session 所有权与失败状态，并保留宿主显式重试入口。

代码位置均指冻结树：

- `sdk-rust/src/client.rs:1211` 的 supervisor 持有 `mutation` 锁，锁覆盖 1243–1283 行整个后台重试循环。
- 858–869 行虽然实现了 `expired + Connect + original_user` 的 fresh 分支，但后台遇到 expired 返回 `ExchangeOutcomeUnknown`，1254–1280 行将其继续退避重试；即使 snapshot 已标记 `AdmissionExpired / retryable=false`，循环仍不停止，也不释放登录所需的锁。
- `login()` 在 664 行先取同一锁；即使另行结束后台循环，668–670 行还会因存在旧 session 且 `initial_login_recovery=false` 而直接报 already logged in，无法进入上述 fresh 分支。正常成功首次登录会将该标志置 false。
- `sdk-rust/src/capability_tests.rs:873` 的已有测试在 884–885 行手动 `stop_runtime()` 并设置 `initial_login_recovery=true`，因此仅验证人为准备后的内部分支，遗漏真实公开入口及后台持锁。

最小真实复现仅使用公开 SDK API 与已有真实 fixture：

1. 通过 `login_with_auth_provider()` 正常完成首次 finite-v2 登录，取得经实际 ES384/JWK 验证、Node/Redis 返回的可信 `Session.user=owner`。没有修改 Client 内部字段或本地墙钟。
2. fixture 建立新的协调 epoch，并在下一次恢复链返回带真实签名、实际提交时被 Redis TIME 拒绝的 expired ticket；首次 register 的错误响应以 fixture 599 模拟丢失。
3. 调用公开 `directory(None)` 发现 epoch 不同，实际触发 supervisor 的 `CoordinationRecovery`。后台精确重放最终得到真实 `REGISTRATION_ADMISSION_EXPIRED`，公开状态成为 `AdmissionExpired`。
4. 关闭 expired fixture 注入，宿主明确调用公开 `login_with_auth_provider("explicit-fresh")`。三秒观察窗口内调用无法完成；唯一 token owner 的调用数仍为 2，记录只有 `[(Connect,false),(CoordinationRecovery,false)]`，没有期望的 `(Connect,true)`。源码所示循环在未关闭 Client 时没有自行退出条件，因此不是一次慢网络响应。
5. `disable_remote()` 则可取消后台所有权，返回 `localClosed=true / coordinatorRevocation=Pending`。此 Pending 正确，不应为了让测试通过而虚报 Confirmed。

私有独立 harness 为 `public-harness/src/main.rs`，实际调用的是冻结产品源码。最终 `cargo run --locked --offline` **exit 101**，失败为显式 fresh 应可达的合同断言；日志 `public-api-counterexample-final.log` 包含状态、owner 调用记录、三秒超时及关闭结果。harness 保留产品锁中的全部实际依赖版本，仅添加私有 root package、去掉产品独有 dev-dependency 边；`cargo update --offline --workspace` 调整后显示没有升级依赖，最终 locked 运行不修改私有锁。详见 `harness-lock-binding.json`。

修复应让明确 expired、此前 owner 已确认的情况结束后台自动重试并释放调度锁，允许宿主通过公开 API 显式请求 fresh，同时保留原 subject/key/产品绑定及旧清理 Pending。不能将 unknown 401、本地时间推断或初次未确认 owner 当成明确过期；不能通过自动刷新或抹掉 `retired_unknown` 来规避本问题。修复后应新增不修改 SDK 内部状态的公开调用链回归。

## 独立验证与保守 Pending 审查

本轮独立执行 `cargo test --locked --manifest-path sdk-rust/Cargo.toml --lib client::capability_tests -- --include-ignored --test-threads=1`，**16 pass / 0 fail / 0 ignored，exit 0**（14 个真实 Node/Redis/JWK 集成用例 + 2 个严格验证用例）。命令、真实耗时与环境见 `commands.json`，完整输出见 `capabilities.log`。没有把开发完整 76/76、clippy、example 或 Lab build 冒充独立结果，也没有在发现阻塞后反复跑无关大套。

通过项覆盖 finite challenge→mint→register→publish→完整目录、两个 Native 同产品、精确未知 mint/register 重放、取消后迟到归属、同 epoch 401 不刷新、不透明票据不被解码为可信身份、可信新 epoch 例外、已知 session 到期 fresh 恢复、未知过期停止网络重放、首次未知 owner 保持 Pending、同 epoch finite 能力删除拒绝降级、legacy、产品迁移关 gate、目录策略 reload 失败 Unknown、provider hash 与 URL 原文/端口/path/limits。正式入口测试实际建立外层 WSS，检查显式 authority/端口及 `/connect`，执行内层真实 mTLS 和 65536-byte smux 半关闭数据链；不是只检查派生字符串。

`retired_unknown` 是有界保守关闭事实。在已确认 owner 的人工准备分支中，明确过期后显式 fresh 成功仍保留此事实，关闭继续 Pending；后续普通已知 session 成功不会单凭“新 session 已确认”清除它。代码仅在未知 pending 通过完整可信配置证实协调 epoch 更换时退役旧域；这不证明旧公开部署物理删除。现有代码因此可能长期 Pending，但没有虚报同 epoch 未知清理完成。本轮不将“Pending 不容易清除”列为缺陷，也不要求放宽合同。R1 指向允许分支无法经公开 API 到达，与该保守事实独立。

Lab 消费兼容改动严格限于 `browser.rs` 两处比较改用 `same_client_binding`，以及测试初始化新增字段；该 helper 验证两份配置、完整 OAuth 绑定及固定产品，忽略已验证能力 metadata，未放宽 issuer/client/resource/scope/OAuth endpoint 或产品迁移。SDK grant owner 两处使用 `same_oauth_binding` 符合 grant 所有权与可更新产品能力分离的合同。

开发冻结树 Lab 日志的 16/17 中失败项是旧 baseline 的 `state::lifecycle_tests::cancelled_rotation_survives_shutdown_timeout_and_blocks_new_login_and_revoke` 固定错误字符串断言；它不在本次 13-path 实现中。root 已合入 `1da15cb` 的新 Lab 生命周期代码。本轮没有改旧树以消除该失败，也没有将旧树失败归咎于新 `browser.rs`。合并后的 root 组合验证仍由调度完成。

## 范围与后续

本轮没有重跑全部原 mTLS/iroh/WebRTC 回归、全目标 clippy、example 或 Lab build；开发相应日志已审计，R1 修复后的受影响实际链仍需独立复核。未等待真实 24 小时；过期票据是已明确标注的签名 fixture，实际经过 Redis TIME 拒绝；已有已知 session 到期测试使用模型 deadline。本轮新增公开反例未修改墙钟或 SDK 内部状态。

只使用本地 loopback、临时隔离 Redis/AOF 和本地 ES384/JWK，无真实 Logto 云登录、WAN/手机、生产、云用户、push 或部署操作。未更改许可或依赖分发边界。

**BLOCKED：需原开发 owner 修复 R1，冻结新 commit 后重新验证公开 explicit fresh、后台停止条件、unknown401/首次未知 owner 的保守边界，以及关闭 Pending 不被抹掉。**
