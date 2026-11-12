# integration-010 受控协调时间独立核对

结论：**PASS，仅限受控时间证据的准确性及本轮覆盖边界**；没有新增 blocking。本报告不将 integration-010 标为完成，也不替代公共装配 r2 报告或实际运行验收。

冻结样例 `141dcd17ad66ef4f1e392be1e32668c321ccaf54` 前后 clean。开发 manifest 列举的 17 个服务/SDK/fixture 模块与冻结树、当前根及根 `360ad87` 内容一致；原 runner、manifest 和 8 份证据 artifact hash 均核对相符。实际执行与私有源码/命令/日志绑定保存在证据标签 `integration-010-controlled-time-independent`，公共源码未修改。

独立原样执行私有 runner，自然 **exit 0，6 PASS、0 FAIL、0 skipped**，其中为五个运行场景及一个安全汇总校验；没有使用 force-exit。本轮逻辑 24 小时场景实际约 **69 ms**，runner 整体实耗约 **1051 ms**。另写三项公开 HTTP/SDK 边界测试，自然 **exit 0，3 PASS**：challenge 在 119999 ms 可 mint；新 mint 封闭旧域；其他有效签名 subject/client 或不同 registrationKey 不能使用原票据；另一个 subject 的首次 console 登录目录隔离；历史协调时间仍拒绝按真实墙钟过期的已签名 JWT。

真实执行边界是现有 `MemoryStore` 原生 `clock` 注入、两个共享该 Store/时钟的 Node HTTP handler、实际静态产品/endpoint 策略和 ES384/JWK 验签。Native 的 challenge/mint/register/publish/revoke 通过公开 HTTP；Console 使用未修改的 JS SDK `connect`、`directory`、恢复和 `disconnect`，没有注入 SDK 私有 session 或改全局时钟。

覆盖结果包括：challenge 精确 120 秒拒绝；已提交 mint 在 challenge 到期后仍精确重放且不延长票据；票据/session 最后一毫秒重放和心跳不延长绝对期限；精确 24 小时拒绝旧 bearer、mint 和 register；仍有效的旧 nonzero-fence challenge 不能在退役后重新进入旧域；同 registrationKey 的新准入保持逻辑 deviceId，但更新 bearer/incarnation 且不继承旧业务 metadata；旧 revoke/publication 不影响新 incarnation。目录在最新心跳后 64999/65000 ms 的 online 边界、bridge 精确 180 秒过滤、heartbeat-only opaque revision 稳定而实际 lastSeen/publishRevision 更新，以及 Native/Console 的 iroh/私有字段投影均实际通过。新建真实 Store 形成新 epoch，旧 epoch/key/ticket 被拒绝；同 subject 的 JS 恢复成功，改变签名 subject 的恢复报 `IDENTITY_CHANGED`。

本轮 Redis 负控使用真实独立 Redis：仅将 handler 注入时间推进 24 小时，Redis TIME 实际只前进 **18 ms**，原注册回执期限不变、challenge 仍可 mint。Redis Lua 的权威时间仍直接来自 `TIME`；MemoryStore 的注入能力不适用于 Redis。没有替换 Lua、复制 Store 或增加产品时钟接口。

JS 首次/恢复 session 均通过现有公开响应校验：协调起点选在真实墙钟前约 24 小时再加 60 秒，使首次到期仍在真实未来，逻辑推进后新 session 也处于 SDK 原有接受窗口。真实 HTTP 记录显示旧目录 401、重新读取配置/console exchange、目录 200。恢复 callback 为 `coordination-recovery`、`forceRefresh=false`，含义是用仍有效的 fixture 身份授权重新交换 console session，**不是 OAuth grant 强刷**。JWT 的签名、iat/exp 仍按真实墙钟验证；没有模拟 24 小时身份 token 老化或官方 refresh rotation。

开发首次 `2 PASS / 4 FAIL` 和第二次 `4 PASS / 2 FAIL` 的原日志及其 hash 保留。错误的 force-refresh、wire cursor、域重放顺序和心跳基准假设已在私有 runner 中修正；没有把旧失败改标通过，也未为其修改产品实现。

因此，这份证据只能称为“受控逻辑时间下真实 MemoryStore/HTTP/公开 JS SDK 的边界验证”。它不证明实际 24 小时、Redis 24 小时回收、Rust SDK/AuthProvider 自然到期恢复、真实 Logto 云授权/持续轮换、外部 Noise/WAN、手机或 30 分钟/8 小时长跑。Native 本轮仅为 HTTP 路径，未执行 Rust；没有重复 Rust 22 项，也没有手机、云账号、生产或部署操作。
