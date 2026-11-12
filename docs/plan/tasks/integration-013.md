---
id: integration-013
scope: final compatibility packaging and public source preparation
status: done
depends-on: [integration-012]
start-condition: integration-010 public sample module independently reviewed and merged
---

## objective

在010公共模块独立通过并合入后，完成实际新Web所需CI、版本/兼容说明、精确源码策略和完整仓库源码包。仅交付发布准备，不操作GitHub发布、正式部署或改写已公开历史；010继续拥有真实云/公网/手机/长跑、最终快照与发布的整体门槛。

本任务从012和010只读发布预检拆出，避免把012现行模块通过当作最终新样例可发布。`start-condition`是010公共模块已通过的检查点，不要求010整体任务done；010最终发布又须本任务完成，不形成整体任务依赖循环。

## context

- `docs/plan/tasks/integration-010.md`及其独立评审、`docs/common-sample.md`。
- `docs/releasing.md`、`docs/metadata-and-compatibility.md`、`docs/deployment.md`。
- `scripts/release/source-policy.json`、`package.py`及实际workflow。
- 私有只读预检标签`integration-final-publication-preflight`，不复制私有内容到源码。

## path

`.github/workflows`、`scripts/release`、根README及发布/兼容/使用文档、下载页面元数据；SDK/服务/Lab的版本元数据及其锁文件。只修改版本和构建/发布边界，不修改SDK、鉴权、Store、provider或业务实现。共同样例源码由010 owner维护，不与其并行修改。

目标project tag为尚未使用的`v0.8.0`，最终发布前重新核对占用；组件目标Rust SDK`0.3.0`、JS SDK`0.2.0`、Lab`0.4.0`、server`0.2.0`，首次共同样例保留`0.1.0`，历史Go SDK保持`0.4.0`。这些是待发布版本，不更改旧tag、旧Release或既有下载的能力说明；版本号本身不代表真实运行验收已完成。

选择**完整仓库源码归档及逐文件manifest**承载共同样例，保留其相对SDK/服务依赖；不新增一个缺少这些依赖的独立sample ZIP。不把外部AnyWork、AGPL实现、私有adapter/wrapper、token、云配置或证据打入归档。项目与第三方许可正文不变，不自动授予SDK嵌入或分发权。

## verification

1. CI在实际Web已存在时执行其`npm ci`、`test`、`test:integration`及build，Redis required；锁检查包含Web锁。sample显式`--workspace`仅为审计清晰，当前virtual workspace默认已覆盖all members，不宣称修复了一个并不存在的漏测。
2. 精确纳入已存在且经审的010/012/013任务和评审文档；`web/.env.example`采用单文件exact例外，通用`.env`、`.env.local`等仍拒绝。实际planner、exporter、scanner和额外二进制/外来文件负控通过；最终新freeze重新导出和隐私/许可核对，不沿用012中间报告。
3. 版本、锁、manifest、archive文件名、release notes及兼容矩阵相互一致；旧v0.7下载不可变。实际运行source打包、解包ESM导入、完整仓库源码解包后的相对依赖/构建命令，核对逐文件hash和源码commit。完整源码包与其他产物均进入实际release manifest及SHA清单，不以硬编码旧产物数量为验收。
4. 源码公开采用实际公开基线上的经审快照提交，记录本地冻结F→导出tree/manifest M/S→公开commit C→Actions与下载/部署映射。只读预检已确认未发布本地history有旧机器信息，不能直接push本地main；当前tree扫描不能证明history安全。保留私有原始证据、不rewrite已公开历史，检查实际待上传blob和commit范围。
5. 独立评审绑定冻结源码及实际命令/exit；只将已运行的平台/构建列PASS。未执行Actions、手机、云账号、Noise、WAN、长跑或正式发布时准确标记。任何此模块通过都不替代010整体门槛。

## execution

发布准备实现及 I013-P2-01 修复已完成，独立 r2 对冻结 `5a7792f` 的候选准备范围评审通过；原 r1 BLOCKED 报告和失败证据保留。所有组件源码及附带许可/文档均共享公开源码策略和二进制边界，原环境文件反例未改字节并实际重跑通过。

小冻结 `5deb038` 仅精确收录实际生成的 `docs/plan/reviews/integration-013-r2.md` 并重做归档、逐文件 manifest、导出和扫描；该冻结阶段保持 `in-progress`，等待独立 private closeout，不创建自引用的公开 r3 报告。此前构建、测试与许可结果按各冻结绑定继承，未重复执行的项目不增加计数。

独立 private closeout 已 PASS，报告在候选、root、独立导出与实际归档逐字节一致。根分支顺序合入 `90b348d`/`a62f477`/`839fb43`/`cf95aca`/`69cbfa3` 后实际组合复测：package 8、policy 7、sample-license 2、sample/server 117/39包许可核验、server/JS/desktop/Web四构建、Web owner 5、真实Node/Redis/JWK/UI集成8，均exit0。根实际生成13个产物，完整源码2,978文件及五个源码归档逐条hash/inventory/内外manifest相等，八个ESM/type产物解包实际import通过；重新plan/export/known-private与binary/text扫描exit0。私有证据标签为`integration-013-root`，绑定根冻结`69cbfa3`；本次未重复Rust运行测试或原生包构建。

任务按发布准备模块标记done。此后的010实际运行发现合成上游echo协议不符，修复和真实业务验收仍归010；新运行源码或文档改变后最终发布必须重新冻结/导出，不能把本次快照当作将来版本的扫描结果。

本模块不执行 push、tag、GitHub Release、Actions 或部署，不证明未发布本地 history 可公开；最终仍须在实际公开基线建立经审快照并核对映射。integration-010 的云、手机、公网、Noise 和长跑整体门槛继续单独追踪。
