# integration-013 r2 独立评审：PASS（候选发布准备）

独立 verify 绑定 `task/integration-013` commit `5a7792f9da206f3c127a26d161fcb94d156391bb`，parent `d4764fb42a4c86781dee02e487110234fb4eb07d`。本轮五个变更路径均匹配开发 source manifest；原评审中的其余 163 个路径哈希保持一致。开始及结束冻结树 clean，本轮未添加依赖链接、未修改实现。

**I013-P2-01 已关闭，无新增 blocking。** 原 `integration-013.md` 保留为旧冻结的 BLOCKED 记录。本 PASS 仅覆盖此次候选发布准备及修复；生成本报告后仍需 exact 文档/policy 小冻结和私有独立 closeout，尚不能声称最终导出已包含本报告，也不能据此发布或认定 integration-010 整体验收完成。

## 修复及原反例

`scripts/release/package.py` 的 `public_sources(*prefixes)` 统一执行 eligibility 和 binary policy。组件源码及其 manifest、Lab、ESM 附带 metadata/notices、完整许可集合、Go native 附带 docs/provider 资源，以及 Mac/Android wrapper 和 finalize 补充文档入口，均改用该共同选择器。生成的 native output 与精确八个 ESM/type output 继续使用各自显式输入边界；没有扩大 source-policy 的 binary 例外。

独立原样执行 r1 的 `component-env-negative.py`：harness 字节哈希与锁定版本一致，实际 **exit 0**，`fullArchiveHasEnv=false`、`componentArchiveHasEnv=false`、`sourceInputUnchanged=true`。旧 r1 fixture、失败日志和 JSON 在隔离重跑后按原字节恢复；r2 输出单独保存，未覆盖原 exit 1 证据。

该修复覆盖统一入口，未采用 Go 路径或 `.env.local` 后缀特例。独立追加的跨组件反例验证如下：

- 15 个禁止输入，包含不同嵌套位置及后缀的 env、私钥类文件、日志、备份、Finder metadata 和排除目录文件；完整仓库、三个组件、Lab、ESM、许可包及合成 native 附带文档均未纳入禁止标记。
- 21 个合法输入保留原字节，包括 CRLF/空白许可文本、组件源码、两个 exact `.env.example`、样例相对依赖声明与 ESM 附带许可。组件 manifest 的条目/哈希与实际归档相同，内外 manifest 字节一致。
- 六处 eligible 外来二进制分别位于 SDK 源码、Rust 许可、JS notices、Lab notices、server notices、sample notices；每处定向 `public_sources` 和实际 `main(sources)` 均拒绝，未通过非 binary 后缀绕过检查。
- 精确八个 ESM output 保留；合成 native output 按显式生成物输入进入 Go 包，合法附带 docs/许可保留。此项是打包边界验证，不是 Mac/Android 原生构建或执行。

独立 harness `cross-component-controls.py` 最终实际 **exit 0**，其源码、输入、产物及 proof 均归档。两个早期 reviewer oracle 错误已单列保留：误把 source root 中允许的普通 UTF-8 文件当成禁止文件，以及误将组件相对 `LICENSE` 与仓库根 `LICENSE` 比较。纠正的是测试前置与路径映射，没有改产品实现或锁定的 r1 harness，两个失败不计为产品 finding。

## 独立实际验证

| 项目 | 实际结果 |
| --- | --- |
| package / policy / sample-license 单测 | 8 / 7 / 2 项，三个命令均 exit 0。 |
| 原 r1 harness / 新跨组件 controls | 分别 exit 0；原失败证据保留。 |
| 实际 `sources` | 仅将输出重定向私有目录，实际 exit 0；13 个产物哈希均与开发记录一致。 |
| 完整与组件源码归档 | 完整仓库 2,977 文件，Go 104、Rust 1,179、JS 18、Lab 2,625；逐文件匹配当前源码、policy 集合、内外 manifest；路径唯一、安全且均为常规文件。 |
| 许可集合与 ESM | 许可集合 2,525 文件匹配当前原始字节和允许集合；ESM BUILD-INFO、source manifest、全部构建哈希及精确八个 outputs 匹配；实际 Node ESM import exit 0。 |
| 相对依赖与 exact 文档 | SDK、server fixture、sample workspace、官方 owner、Web integration 输入齐全；实际 `integration-010-time.md` 和 r1 review 与 root 原报告逐字节一致并已 exact allowlist。 |
| plan / export / scan | 三个实际命令均 exit 0；当前导出 2,977 文件，逐项 manifest，已知私有值及 binary/text literal 扫描无 findings；显式 env 注入负控被 scanner 拒绝，输入未改，清理后快照 inventory/哈希恢复原样。 |

补充 scanner 实验遇到初次扫描后生成的 `.DS_Store`，扫描正确拒绝该文件；该次混合实验没有被计为“普通外来文本文件被 literal scanner 拒绝”。外来 inventory 的失败关闭由 exporter 和已运行 policy/package 测试验证。生成 metadata 的原字节、诊断日志及快照恢复核对均保留。

r1 已独立完成的 CI/版本/asset set、117 个 sample npm 与 39 个 server npm 许可检查，以及 Mac registry 501 个依赖的许可来源核对，对本轮未变输入继承，不计入 r2 新运行数量。原解包 Node builds、Web Redis/JWK integration 和 Rust offline 检查同样仅继承未变源码/锁的既有证据；本轮未重复 Rust 22/78 项或原生构建。既有上游许可正文未归一化，许可来源与分发权边界不变。

## 收尾及限制

本报告尚未存在于冻结候选，因此未预先允许不存在的 r2 文档。后续应将本报告实际字节 exact 纳入 policy，在小冻结上重新导出、扫描并做私有独立 closeout attestation；无需新增公开 r3 报告形成自引用。

当前 tree/快照扫描不证明未知秘密或历史安全。仍须从实际公开基线形成经审快照，检查真实待上传 blobs/commits；不得直接 push 存在旧机器信息的本地未发布 history。真实云、公网、Noise、手机、自然长跑及 integration-010 整体门槛不在本轮通过范围内。

私有证据标签 `integration-013-independent-r2` 保存冻结/source 绑定、原反例重跑与 r1 恢复记录、独立新 harness、实际产物与导出、命令 exit、proof、公开报告绑定及 `SHA256SUMS`。本轮没有 push/tag/Actions/deploy/history rewrite，没有云/手机操作，没有新增 Rust target/toolchain。
