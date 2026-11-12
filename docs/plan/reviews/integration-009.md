# integration-009 独立验证

状态：**PASS**。绑定冻结提交 `31b3c6629af2cc0e310e2855fcdc384e1815c27c`，base `04e43f633d9e1a9854fe5e338bfecb102a36d9ee`，开发工作树 clean。唯一修改文件为 `scripts/release/source-policy.json`，SHA256 为 `c6e0bee3a42a05b0e6a9f77c6cf7d9c7fb5306395503355e6098a95746311c89`。

## 差异与公开内容

独立核对 JSON 差异只向 publicDocs 追加 19 条现存文件路径：五份集成/准入/桥接/宿主控制/元数据合同，两份整理后的分析与进度文档，integration-001 至 integration-009 九份任务，以及 integration-001 至 integration-003 三份评审。无删除、重复或通配 docs 扩大；sourceRoots、私有目录/扩展名排除、精确二进制例外、扫描器及上游 fixture 白名单均未改变。

已阅读这 19 份冻结文档并核对其 Git commit 字节。内容为公开技术合同、任务和脱敏验证结论，明确区分计划、源码、本地 fixture 与实机/生产能力；没有把用户原附件或私有逐条证据纳入发布。评审中仅保留工作区外证据的相对归档名称，不含本机绝对路径、真实身份、token、票据或密钥。

## 实际导出与独立检查

开发方已实际执行完整 plan/export/privacy scan，导出 **2,739 文件、218,145,645 字节**。独立审阅者重新执行 plan，亲自遍历现有导出目录，核对完整路径集合、每个文件字节数及 SHA256，并与冻结工作树源文件逐字节比较；开发 source-final 中绑定的报告哈希全部一致。按原 exporter 的 Path 排序算法重算目录树摘要得到 `012458ea1be12ac5f0676cd8972c58489158c7bbc532b591c50949d2db3b794c`，与原 manifest 相符。未新增重复大二进制扫描或改写 provider 原件。

独立另外导出本次新增的 19 份文档，运行未修改的隐私扫描器及已有私有值集合，结果 **19 文件、0 findings**；私有值只由扫描器读取到内存，没有输出值或其摘要。原全量扫描的 2,739 文件、0 findings、六个精确固定上游 fixture 例外及合成负面 fixture 拒绝结果，均通过原报告哈希绑定；这些不冒称为本轮重新执行的全量扫描。

直接调用真实 eligibility 逻辑的 11 个负面路径检查全部拒绝，覆盖项目 .env、Vercel 本地配置、credentials/private-device、日志、原始反馈附件、SSH 和归档资料，以及此基线之后的评审/任务文件。实际导出集合也未包含这些私有类别。`git diff --check` 通过。

## 范围与后续门禁

本 PASS 仅批准此冻结基线上的 19 条公开文档 allowlist 改动，不是最终全项目发布批准。后续 integration-004/009 评审、integration-010 及其他新增任务或合同必须另行明确审阅、补 allowlist，并在最终发布源码上重新导出和扫描；本次未偷偷纳入尚未冻结的新文件。字面量/文件类别扫描并非完整隐私证明，公开内容审查与发布历史审查仍有独立责任。

独立证据保存在工作区外 `integration-009-independent/review.json`、`plan.json`、`new-docs-scan.json`；开发全量证据仍在 `integration-009-development`。本轮未修改政策或扫描器、未 Git commit/push、未部署或操作任何用户设备/授权。
