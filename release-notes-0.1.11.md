## 0.1.11 — 2026-11-22

- 每次重新打开或重新显示 SVN 面板时，自动读取一次当前工作副本状态。
- 定时自动刷新仍默认关闭；保持面板打开和保存文件不会因此开启轮询。
- 忙碌时延后刷新；与初次加载、正在进行的刷新及操作完成后的刷新合并，避免重复请求。若面板再次隐藏，等下次显示时再处理；读取失败也不会连续重试。
- 此次刷新针对当前工作副本状态，保留提交说明、文件勾选和目录视图；文件勾选仍会按最新的可提交状态校正。
- 验证：71 项 Rust 测试和 16 项前端测试，共 87 项测试通过；静态检查、Wasm 构建和官方签名安装包校验通过。新增测试覆盖重新显示、初次加载去重、忙碌时延后、再次隐藏及失败后不循环重试。

要求 OpenIDEA 插件 API 1.10 和已安装的 SVN 命令行客户端。沿用原插件 ID、发布者签名密钥和权限。Windows 真机界面及远程开发主机尚未验证。

English summary: Refresh the current working-copy status once whenever the SVN panel is reopened or revealed. Periodic refresh stays off by default. Busy operations defer one pending refresh and coalesce it with startup, an in-flight refresh or operation completion; hiding again defers it and errors do not loop. 71 Rust and 16 frontend tests passed, with official signed package validation.

SHA256: `8fc8e190fbf82962612e267779a00caeb6445c0703fad8b63b04748a53552826`
