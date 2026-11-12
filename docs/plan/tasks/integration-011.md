---
id: integration-011
scope: ram-only desktop owner and local workspace sample modules
status: done
depends-on: [integration-003, integration-008]
---

## objective

实现共同样例的Mac Tauri桌面授权owner、本地受限Web与独立Gateway业务上游模块，让本地业务在未登录、云端不可达或关闭远程时继续可用。作为integration-010的模块前置，可与006/007的SDK改动并行；不把本模块通过视为完整远程样例、真实云登录或长跑通过。

## context

- `docs/common-sample.md`：唯一模块/创建链、安全边界与UI合同。
- `docs/integration-contract.md` 第2/3节；SDK实际AuthProvider与默认remote gate接口。
- `docs/lab-browser-login.md`：仅参考官方SDK与系统浏览器边界，不能沿用文件grant交接。
- `docs/metadata-and-compatibility.md`：RAM-only、许可与平台边界。

## path

`examples/ram-only-workspace/`中的desktop、local-workspace、gateway-upstream及必要共享类型/构建/本地页面/README。只改此样例目录，不改SDK、Lab、服务、根文档、外部AnyWork或生产。外部桥接装配与私有证据仍在源码树外；不能新增AGPL依赖或复制其代码。

## implementation boundaries

1. 官方`@logto/client`内存adapter为唯一刷新owner；Tauri Rust仅通过带上下文AuthProvider获取access token。准确接口先报root对照合同。未知轮换由owner保留且停止重放；取消不能丢失已派发操作。固定OAuth/产品绑定、单飞、owner/SDK generation和主WebView IPC检查均需实际实现。
2. 系统浏览器/loopback callback、可信transport可以参考Lab，但不调用其CredentialStore、browser_commit、journal或私钥/凭据文件路径。只允许非敏感安装ID持久化；新进程需重新授权/开启、换公钥不自动继承信任。
3. LocalWorkspace与Gateway upstream使用独立loopback listener，共享同一RAM合成业务状态，分别执行共同样例合同的cookie/Origin/CSRF与内部RAM bearer规则。静态页面不包含私有状态；没有账号时也可由desktop一次性授权打开本地Web。
4. remote gate默认关闭。宿主复合关闭需先封闭本机gate、保留并驱动所有已有清理future，不能停止本地listener/既有本地连接。对外部Gateway的控制接口只能定义真实可注入的生命周期边界；没有实际Gateway实例时明确未安装/不可用，不使用假成功代替远程可用。
5. 本模块不实现或冒充Noise协议、业务授权、bridge发布及远程Web。006/007合入后由010装配实际provider/Gateway/adapter，完成双端登录、取消/失效、手机与长跑；若本模块新增接口无法被真实外部API实现，必须在合入前修正。

## verification

实际Tauri宿主构建/typecheck及模块单元/真实loopback HTTP/WS反例：未登录打开本地；兑换一次性/超时；Host/Origin/CSRF/cookie与跨端口/重绑定拒绝；内部Gateway bearer与浏览器入口不能互用；远程关闭后同一本地TCP/WS继续，内部bearer失效；慢token请求、取消/迟到回调与重复关闭不丢owner或重新开门。官方SDK真实实例+受控OAuth HTTP装配验证RAM存储、minValidity/forceRefresh/单飞与未知轮换，不以自制token owner替代官方实现。证据绑定冻结commit及实际退出；真实云授权单列未验证，之后另人独立审查。

## delivery evidence

初次冻结`93798e9`的queued enable/priority disable竞态已在修复冻结`ee96a17`闭环，[独立r2评审](../reviews/integration-011-r2.md)PASS（仅011模块）。根分支顺序合入为`7ee1acf`、`f3b3a05`，与当前006/007组合后：workspace Rust **17/17**、官方Logto实例JS **5/5**、前端typecheck/build及原生locked build均exit0，零跳过。根日志/命令/hash存于私有证据标签`integration-011-root`。

独立五项实际host场景含三项新反例，覆盖等待锁的旧enable早拒、held bind/state时同步封真实TCP/bearer、取消等待仍保有cleanup及同本地TCP继续。原失败harness无条件等待已取消入口，不能表达正确early reject；原exit101证据保留，未伪标原样PASS。默认factory仍Unavailable，实际descriptor/Gateway/远程Web、云登录/手机/长跑归010，不能以本模块done结案。
