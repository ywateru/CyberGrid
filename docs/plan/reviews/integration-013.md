# integration-013 独立评审：BLOCKED

评审角色为独立 verify，不修改开发实现。冻结分支 `task/integration-013`，commit `d4764fb42a4c86781dee02e487110234fb4eb07d`，base `1d30fd1715d7f0a632b1aa6fac48287c7af2df9a`。开始及结束均 clean；166 个变更路径在两次绑定中均匹配开发 source manifest。临时添加的四个依赖链接已移除。

本结论仅针对候选发布准备，不能据此发布或认定 integration-010 整体验收完成。存在一项 P2 blocking，需修复后独立复审。

## I013-P2-01：组件源码包绕过完整仓库源码包的公开策略

- **Severity：P2；blocking：是。**
- **合同：** `docs/plan/tasks/integration-013.md` 第 28、33 行要求排除私有配置，只有经审的 `.env.example` 是 exact 例外，通用 `.env`、`.env.local` 仍拒绝；`scripts/release/source-policy.json` 明确环境文件及本地冻结资料不得公开。该边界适用于同次 `sources` 生成的各组件资产。
- **代码：** `scripts/release/package.py:22` 的 `public_sources()` 对完整仓库执行 eligibility 与 binary allowlist；但 `package_sources()` 第 129 行直接以 `tracked(prefix)` 打包 Go、Rust、JS 组件，第 130 行的 `source_map()` 同样读取未过滤文件。第 144 行 Lab 和第 149 行许可集合也直接使用 tracked 路径，未共享这层公开策略。
- **实际反例：** 私有 clean 合成 Git 仓库中仅添加无秘密的 tracked `sdk/.env.local`，调用冻结的实际 `package.py` 模块及其 `main()`，仅将输入根和输出目录指向合成仓库。该命令仍执行 clean 检查、provider 校验与完整 `sources` 流程。生成的 `CyberGrid-0.8.0-source.tar.gz` 排除此文件，但 `cybergrid-sdk-0.4.0-source.tar.gz` 包含 `.env.local`，输入字节未变。独立断言实际 **exit 1**。
- **最小重现：** 在合法、clean、已构建精确 JS outputs 的测试输入中提交合成 `sdk/.env.local`，执行 `python3 scripts/release/package.py sources`，比较上述两个 tar 的条目。应同时排除或拒绝该私有输入；当前只有完整仓库归档排除。
- **影响与修复要求：** 当前冻结候选未发现实际秘密泄漏，但 tracked 不能作为公开许可。所有组件源码、Lab、ESM 附带源码和许可资产应采用一致的 source-policy 或明确且等价的允许集，并对二进制执行对应边界；不能仅给 `sdk/.env.local` 加后缀特例或放宽 policy。回归同时验证其他禁止环境文件、外来文件及合法源码/许可完整性，保持 manifest 与归档条目一致、相对依赖可用。

原 harness、原始失败日志、输入仓库及实际输出归档保留于私有证据标签 `integration-013-independent`：`component-env-negative.py`、`component-env-negative.log`、`component-env-negative.json`、`component-env-fixture`。其原始字节哈希及实际 exit 已绑定，复审不得改写该反例以获取 PASS。

## 已独立完成的核对

| 核对 | 实际结果及边界 |
| --- | --- |
| package / source-policy / sample-license 单测 | 分别 6 / 7 / 2 项，三个命令均 exit 0；包含外来输出、精确允许集与失败关闭检查。这些通过未覆盖上述跨归档不一致。 |
| npm 许可生成物检查 | sample 117 包、server 39 包，分别实际 `--check` exit 0。 |
| 实际 `sources` | 仅输出重定向私有目录，exit 0；独立生成 13 个产物，哈希均与开发绑定相同。 |
| 完整仓库归档 | 2,975 个文件逐项匹配冻结源、外置 manifest 与内置 manifest；路径唯一、常规文件、无越界条目，相对 SDK/server/owner/sample 依赖存在。环境文件仅两个 exact `.env.example`。 |
| ESM | 实际解包 `index/bridge/directory/endpoints` 的 JS 与 d.ts 共 8 文件，BUILD-INFO 源码 commit、source manifest 与构建哈希匹配；实际 Node ESM import exit 0。 |
| Mac 许可图 | installed stable 的 locked/offline Cargo metadata exit 0，未编译；501 个 registry package 精确 name/version 均有索引覆盖，923 份文本哈希匹配，其中 882 份直接匹配 registry 原文字节，41 份属于有记录的 fallback，未称为 registry 原文。 |
| 既有许可正文 | base 中 2,338 个 notices/licenses 路径未修改；新增上游正文保留字节与空白。排除上游许可文本后的 authored diff-check exit 0，未通过归一化文本消除 whitespace 提示。 |
| workflow | 两个 YAML 实际解析 exit 0；源码审查确认 required Redis、实际 Web integration、锁文件安装、SDK/sample `--include-ignored`、sample workspace、两个 JS HTTP 链及 release 对 integration 的依赖。未运行远端 Actions。 |

独立检查了版本元数据、兼容说明、版本派生 asset set、manifest/provenance 与源码策略的实现；未见第二项 blocking。开发提供的解包后四个 Node build、Web 真实 Redis/JWK 8 项与 Rust offline 检查证据已核对其冻结绑定，**没有计入本轮独立运行数量**；已知 blocking 后未重复这些构建或不受修改影响的 Rust 22/78 项。

许可 fallback 按记录区分 pinned upstream、同版本 wrapper 与标准许可正文。既有 `enum-assoc` 记录明确表示选择其声明的 Apache-2.0 分支、标准正文不是 upstream original，未把近似版本许可文本冒充精确原文。此核对不是法律授权意见，不扩大原有项目/第三方分发权。

## 后续门槛与证据

当前候选尚未包含本次 review 和后续已产生的 integration-010 受控时间报告；这是 task 已明确的收尾阶段，不能宣称 final export 已覆盖。修复后需将实际获准文档精确纳入 policy，重新冻结、导出并做独立 delta 核对。

本次没有证明本地未发布 history 可直接 push。开发只读预检已记录旧历史中的机器信息；发布仍需采用实际公开基线上的经审快照并检查实际待上传 blobs/commits。完整源码当前 tree 安全检查不替代历史审核。

私有证据标签 `integration-013-independent` 保存 `freeze-before.json`、`freeze-after.json`、命令 ledger、原始日志、`archive-independent-proof.json`、`independent-license-audit.json`、公开报告绑定及 `SHA256SUMS`。本次无 push/tag/Actions/deploy/history rewrite，无新增 Rust target 或 toolchain 下载，无云账号、手机、真实 WAN/Noise 或自然长跑；Mac/Android 构建和 integration-010 整体门槛均不因本报告获得 PASS。
