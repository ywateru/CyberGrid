---
id: integration-009
scope: public source export policy
status: done
depends-on: [integration-001, integration-002, integration-003]
---

## objective

把当前已经审阅的公共集成设计/任务/评审文档加入显式源码导出策略，避免 GitHub source export 静默遗漏新的合同和保留说明。只修订必要 allowlist；继续排除本地用户信息、私有日志/证据、凭据与孤立目录。

## delivery

冻结提交 `31b3c6629af2cc0e310e2855fcdc384e1815c27c` 已独立PASS，以 `2a135a8` 合入；根分支确认 JSON、35条公共文档路径存在且不重复。见 [评审](../reviews/integration-009.md)。只覆盖该冻结基线，后续改动仍须最终导出/扫描。

## context

`scripts/release/source-policy.json`、`docs/releasing.md`、`docs/INDEX.md`，以及本次新增集成文档。此任务是发布准备，不代表后续功能或最终发布已完成。

## path

`scripts/release/source-policy.json`；只有实际验证发现需要才改相关 export/scan 脚本。不改 SDK/服务/Lab/根设计文档，不 push/deploy。产物和私有扫描报告放工作区外。

## verification

实际执行现有 source plan/export/privacy scan，证明公共新文档和 source files 纳入、外部归档与本地配置不纳入；审查导出目录、manifest 和本次改动，不能把用户原附件或私人 evidence 复制为公共文档。保留 token/secret 扫描规则，不通过豁免真实凭据来通过。冻结提交和有界证据交 root；由另一人审查后合入。后续新文件/最终源码仍须在真正发布前重跑检查。
