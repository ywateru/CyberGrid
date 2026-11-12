---
id: integration-008
scope: tauri explicit remote controls
status: done
depends-on: [integration-003]
---

## objective

使 Tauri Lab 显式消费 Rust SDK 分层快照和远程 gate，登录后默认不发布端口，明确开启/关闭，不将远程关闭等同于退出授权或停止本地业务。Android 使用既有 debug ADB 控制新增开关，不使用输入 token 的方式。

## current finding

开发中的真实 host→SDK→HTTP/WebRTC+mTLS 链路曾在 disable 等待超过60s。开发定位为宿主暂停SDK关闭future后，先等待使用同一peer锁的Mapping.close，造成清理顺序死锁；已作宿主窄修，不涉及SDK改动。实际流开发回归已验证离线关闭、双向EOF、本地同TCP继续与同owner新Client重开。需分别记录 gate 封闭、活跃流终止、调用/UI等待截止，不以已有SDK局部测试PASS结案；冻结与独立结果见下一段。修复跨SDK边界时由对应任务处理，不在本任务越界修改。

首次冻结 `6237eb08b8a646aecabc2cdd77dd2433fae7779c` 开发25/25通过，独立复核实际流路径通过，但实际ADB文件循环发现两项blocking：I008-P2-01 在dispatch等待云撤销时status.json仍显示旧开启状态；I008-P2-02 refresh等待HTTP时mailbox不能受理disable，关闭受慢云操作阻塞。两项均须在原任务修复并重新冻结/独立复核，不通过放松epoch/sequence/幂等或丢弃在途future解决。当前未合入、未实机或部署。

replacement完整冻结 `d953b61371e3e8b09487e9144d4a24605356618b` 已通过[独立复审](../reviews/integration-008-r2.md)，独立7项实际host场景及Python5项断言PASS，两项原阻断已闭环。首次BLOCKED报告作为历史保留，不适用于新冻结。实际文件循环/原CLI与扣留HTTP验证priority disable、live状态、pending重复和高水位；WebRTC/mTLS双向流关闭后同本地TCP继续。开发Python22/22、host28/28及严格Clippy/build通过；合入 `1da15cb` 后根分支host28/28、Python22/22、前端构建/状态断言及browser-start2/2通过。没有Android实机、云登录、WAN或部署结论。

## context

- `docs/lab-remote-controls.md`：界面、宿主生命周期及 ADB 合同。
- `docs/integration-contract.md` 第 2/3 节。
- integration-003 最终冻结 SDK 的实际 API / README。

## path

- `apps/tauri-test/src`、`src-tauri/src`、`tools`、README 及必要的宿主 crate 配置。
- 不修改 SDK、服务端、根文档、生产凭据或用户手机授权。

## verification

前端类型/构建，宿主 locked Rust 构建/测试，debug-only ADB parse/dispatch/CLI 反例。真实 host→SDK→service 与本地业务流验证登录未发布、显式 enable、离线/在线 disable、既有流关闭且本地业务继续、重开与迟到结果隔离。不能仅看按钮字符串或 mock 返回值；若需要修改 SDK 才能满足，先报 root 协调，不在任务内越界改 SDK。
