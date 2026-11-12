---
id: integration-012
scope: public documentation console and repository CI
status: done
depends-on: [integration-006, integration-007, integration-008, integration-009, integration-011]
---

## objective

统一网站、静态文档、部署按钮及仓库CI与已验证的产品/SDK合同，修复console失败仍保留旧online及产品切换迟到响应。为010完整样例验收和最终发布提供真实构建/验证入口，不发布或部署，不提前宣称共同样例、云登录、手机或长跑完成。

## context

- `docs/plan/tasks/integration-010.md`的known publication gaps。
- `docs/deployment.md`、`docs/browser-bridge.md`、`docs/integration-contract.md`。
- `docs/metadata-and-compatibility.md`、`docs/releasing.md`及已合入SDK README。
- `docs/common-sample.md`：样例和外部实现/许可的边界。

## path

`server/public`的网站/console及对应静态文档、`server/test/console.test.ts`等直接相关网站验证、根`README.md`、`docs/releasing.md`与`docs/metadata-and-compatibility.md`、`.github/workflows`、`scripts/release`的源码策略/必要打包入口。不得改SDK、服务鉴权/Store、Lab、样例源码或外部AnyWork，不触生产/GitHub发布。010负责`examples/ram-only-workspace`的完整装配和最终冻结后的导出/发布验收，两任务不能互相覆盖代码。

## verification

1. 主页仍以文档、SDK下载和管理入口为主，静态产品策略是推荐配置，Management参数为可选动态模式；Vercel按钮与正文、实际必需环境变量一致，public origin保持可选。旧版下载描述保留其实际能力，新能力绑定新版本和最终产物后才能展示为已发布。
2. 实际console控制流在网络/权限/解析/容量错误时先撤销可用结论、显示unknown；切产品、退出、重新登录和并发读取的旧响应不能覆盖当前产品。错误提示不泄露原始凭据/服务响应。需独立可复现的状态/迟到响应反例，不能只改横幅。
3. 仓库CI安装真实Redis及Node依赖、使用`REQUIRE_REDIS=1`，运行Rust真实仓库集成（`--include-ignored`）、JS完整单测及新旧HTTP集成；新增样例构建/官方owner及实际loopback验证入口。完整仓库验证不能被独立Rust源码包的默认ignored行为掩盖。workflow触发范围覆盖相关服务/SDK/样例/策略文件，依赖锁不漂移。
4. 公开CI不依赖/下载/打包私有AnyWork装配，不把生命周期替身当作实际Noise证据。公开源码策略补齐已审合同、任务/评审及样例路径，生成文件/本地隐私继续排除；公开评审用分支、仓库相对路径及私有证据标签，精确本机路径与原始报告hash留在私有证据。当前冻结导出与扫描通过。010最终源码冻结后仍须重新导出、扫描与核对产物，不沿用本任务中间报告。
5. 实际构建网站、SDK及相关CI命令；文档/按钮/console的独立验证绑定冻结提交，未运行的Actions/平台如实标记。只有010达到整体门槛后才按原授权发布部署。

## preflight findings

只读预检确认`package.py`仍只允许JS的`index.js/index.d.ts`，新SDK实际有`index/bridge/directory/endpoints`四模块及声明；必须使用精确八产物清单，保持ESM归档、manifest和实际导入一致，不能改成任意dist通配放行。当前冻结实际执行`package.py sources`并在归档中验证四模块/声明和无外部源码，而非只运行npm pack dry-run。

站点主页和静态docs仍要求Management参数，而README的静态产品按钮已经正确；修站点以README/部署合同为准，不把README改回旧模式。console及callback两份页面壳同时核对。现有`server/test/console.test.ts`已执行实际shipped script，可用于产品切换/失败unknown/迟到响应反例。

## frozen review

初次实施冻结`b605ba9f2901d3828eae94cd228b666f58e62082`，独立[评审](../reviews/integration-012.md)为**BLOCKED**，唯一P1为I012-P1-01：较新应用目录HTTP403已显示unknown后，较早设备请求的迟到成功仍恢复online。反例通过实际脚本PKCE/本地ES384验签callback链进入会话，actual exit 1；原脚本字节和失败结果保留，该冻结未获合入或发布许可。

其余必要独立检查通过：现有console24项、网站build、新旧实际HTTP、八文件ESM实际打包/导入/provenance、policy实际负控及workflow配置。开发完整套件的计数与独立复测分开；未运行Actions/云登录/手机/长跑。Web路径在此冻结尚无源码，只记pending；010最终补齐后重新验证。最终发布仍受010冻结导出、历史检查及实际运行门槛约束。

修复冻结`44cc631318a1f790ddc5bcc2c7047c5bf49a444f`已通过[独立r2复审](../reviews/integration-012-r2.md)：原反例原样1/1、受影响suite32/32及四项独立交叉请求场景均exit0，I012-P1-01关闭。根分支合入`2ce0dc1`/`e97ad1e`后实际服务145/145、JS59/59、新旧HTTP各1/1、Rust SDK78/78（include-ignored、零跳过）、现有样例17/17和owner5/5、严格clippy及前端/原生构建全部通过，policy五项通过。记录为012模块完成；010新Web、实际桥接与最终发布门槛仍未完成，原BLOCKED报告保留。
