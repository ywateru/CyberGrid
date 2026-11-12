# integration-012 r2 独立复审

结论：**PASS**。冻结 commit `44cc631318a1f790ddc5bcc2c7047c5bf49a444f` 关闭 **I012-P1-01（原 P1 blocking）**；本轮没有新的 blocking 或 non-blocking finding。原 `b605ba9` 的 BLOCKED 报告、反例字节与 exit 1 证据保留，不改写历史结果。

## 冻结绑定与范围

- 分支 `task/integration-012`，parent `b605ba9f2901d3828eae94cd228b666f58e62082`。
- 使用 root 当前 task、`docs/common-sample.md`、桥接及集成合同，按 docs-sprint 独立 verify 角色执行；原 unknown、产品上下文及迟到响应合同没有放宽。
- 完整审查三路径 diff：`server/public/console/console.js`、`server/test/console.test.ts`、新增 `server/test/console-cross-request.test.ts`。三项 source hash 均匹配 r2 清单，原任务其余 17 路径 hash 不变。
- 独立证据标签 `integration-012-r2-independent`，见 `INDEX.md`、`commands.json`、`freeze-binding.json`、`freeze-after.json`、`SHA256SUMS`。精确机器路径、原始证据 hash 与 fixture 数据仅保存在私有 archive；公开报告不收录这些内容。
- 未修改实现或原 harness。仅临时链接已有 server 依赖，结束前已移除；冻结树 clean、HEAD 未变，所有实际验证进程已结束。未使用 Cargo target、clean、新 worktree、push 或部署。

## I012-P1-01 关闭依据

原合同见 task verification 2、`docs/common-sample.md`「远程 Web 授权与页面」及 `docs/browser-bridge.md:69`：目录失败先清可用结论；旧响应不能恢复已经失效的用户/产品上下文。

`server/public/console/console.js:58` 在每次 catalog 读取开始时同步清除 catalogReady、推进 deviceRevision 并取消旧设备读取；该操作发生在第一个异步等待之前。`:68` 仅在当前 catalog 完整解析并提交后允许继续读取。`:75`—`:79` 将设备读取绑定到 catalog revision、catalogReady、当前产品成员资格、选择 revision、产品值和设备请求 revision。因此，较旧设备成功、未完成或失败的 catalog，以及已移除产品均无法取得提交资格。注销和 pagehide 同时清除 catalogReady。

原独立 `console-cross-request.test.mjs` 按已锁定字节 **原样执行**，**1 PASS / 0 fail，actual exit 0**。该流程仍经过实际 shipped 脚本的 beginLogin、PKCE/state/nonce、本地 HTTP 与真实 ES384/JWK callback；没有 seed session 或修改源函数。较新的 catalog HTTP 403 后放回旧 devices 成功，最终保持“状态未知”。原失败日志与原 harness 的 hash 在本轮前后均不变。

新增八项开发测试也使用实际脚本、本地 HTTP、签名 callback 和忽略 abort 的迟到响应；既有 24 项测试仅调整原测试 fixture，使其明确具备已验证 catalog，未用该准备方式替代关键公开登录链。

## 独立实际结果

| 验证 | actual 结果 | 私有证据 |
| --- | --- | --- |
| 原独立反例原样复执行 | 1 PASS，exit 0 | `original-counterexample.log` |
| 受影响完整 console suite（24 既有 + 8 新增） | 单次 32 PASS / 0 fail / 0 skip，exit 0 | `affected-console-32.log` |
| 本轮独立新增交叉请求负控 | 4 PASS / 0 fail，exit 0 | `independent-cross-controls.test.mjs`、`independent-cross-controls.log` |
| 实际网站构建与 TypeScript noEmit | exit 0 | `server-build.log` |
| 冻结 diff whitespace | exit 0 | `diff-check.log` |

独立新增四项仍走真实脚本登录链，不 seed 内部 session；只扩展私有 HTTP fixture 的响应控制，原反例文件保持不动：

1. catalog 网络连接失败后，旧 devices 成功和 timer 均不能恢复在线；实际刷新按钮重新验证 catalog 后可恢复新读取。
2. catalog 尚在读取时禁止设备派发；随后同产品 catalog 成功也不能重新授权较旧设备响应，只有新派发的设备读取可恢复在线。
3. 较旧 catalog 成功迟到，不能覆盖较新的 catalog 403；timer 不发设备请求，显式刷新可恢复。
4. catalog 从 Core 变成非空 Other 集合后，旧 Core 响应、旧选项和 timer 均不能派发或恢复 Core；当前合法 Other 可发起新读取。

32 项完整 suite 另覆盖权限、畸形 JSON、schema、服务端 typed capacity、本地超大 body、catalog pending、同产品成功、空目录移除及实际刷新恢复；既有用例保留切产品、重叠设备请求、退出/重新登录、epoch recovery 和安全固定错误文案检查。三组运行有行为重叠，分别记为 1、32、4，不声称新增了 37 个互不重叠的验收边界。

## 继承证据与限制

原任务其余 17 路径逐项 hash 不变，网站/部署说明、八文件 ESM 精确打包规则、source policy 与四份 workflow 的原独立审查和实际检查可以继承；本轮未重跑不受影响的 Redis/JS、Rust/sample、打包和 policy 大套。原打包产物及 provenance 仍属于 `b605ba9`，没有把旧产物的 commit 改写为 r2，也没有以本轮局部复审代替最终发布重新打包。

新 Web 不在本修复中；012 的条件 CI 入口不等于 010 完整样例、Web 或最终公开快照 PASS。010 最终冻结后仍须核对实际 Web test/build、源码导出/隐私/许可、产物和发布映射。

本轮使用实际脚本与最小 DOM 壳、本地受控 OAuth/HTTP fixture；不是完整浏览器平台、真实 Logto Cloud、生产、手机、WAN、Noise 或长跑验收。未执行 GitHub Actions，没有相关云或用户操作。

**合入门槛：本冻结 r2 PASS，I012-P1-01 已关闭；010 整体交付与最终公开发布门槛保持独立。**
