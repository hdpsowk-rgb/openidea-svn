## 0.1.10 — 2026-11-20

本次为稳定性更新，包含 0.1.9 尚未单独发布的中文提交说明修复。

### 修复

- **中文提交说明**：提交时明确指定 UTF-8，避免系统区域编码或 SVN `log-encoding` 配置将中文、多行说明和 emoji 写成乱码。
- **SVN 字符编码**：普通读写命令也使用 UTF-8 进程环境，减少中文路径的字符转换错误；无法处理时给出明确提示。
- **日志查看范围**：修复读取过程中切换项目日志、以及切换失败后刷新范围与已显示记录不一致的问题。
- **日志分页**：历史结束后停止加载；达到 2000 条保留上限时明确提示，避免显示无法再打开差异的记录。需要查看更早记录时，可通过文件历史缩小范围。
- **复制文件差异**：复制新增文件与复制来源的 BASE 内容对比，避免将全部内容错误显示为新增。
- **忽略规则**：中文 `svn:ignore` 内容明确按 UTF-8 写入；修复未闭合 `[` 等规则与 SVN 实际匹配结果不一致的问题。
- **工作副本操作锁**：修复同一工作副本的另一个子项目在新面板中绕过操作锁的问题。
- **特殊文件差异**：通过 SVN 属性识别已跟踪的特殊文件和符号链接，明确提示当前支持范围，避免将链接目标文件的内容当作链接本身。普通文件及已删除文件的 BASE 差异仍可正常查看。

### 验证与兼容

- 71 项 Rust 测试、15 项前端测试，共 86 项测试通过。
- 使用真实本地 SVN 仓库验证中文说明、忽略规则、复制与删除文件、符号链接等场景；格式检查、静态检查、Wasm 构建及官方签名安装包校验通过。
- 插件 ID、发布者签名密钥和权限保持兼容，要求 OpenIDEA 插件 API 1.10 及已安装的 SVN 命令行客户端。
- 保留内置文件图标、树状/列表视图、目录双击、忽略/取消忽略、确认后还原及默认关闭的自动刷新。

### 已知限制

- 已经写入仓库的乱码说明不会自动修复，需要原始说明另行更正。
- 当前宿主只能添加环境变量，不能覆盖已有变量；若 IDE 已强制设置非 UTF-8 的 `LC_ALL`，仍需调整 IDE 的启动环境。
- 当前宿主会解析符号链接且没有读取原始链接目标的接口。未跟踪/已忽略项目只能提供宿主返回的内容预览，无法可靠区分普通文件和符号链接。
- Windows 真机界面及远程开发主机尚未验证。

English summary: Includes the UTF-8 commit-message fix from 0.1.9; fixes history scope/pagination/retention, copied-file local diffs, UTF-8 ignore rules, APR bracket matching and shared working-copy locking. Tracked special nodes are detected through SVN metadata instead of the canonicalizing host stat. 86 tests and official signed package validation passed. See the known limitations above.

SHA256: `2f06bc2ed2808173b8c740833b039ada2f09abb4286b5a3751b575198244ca92`
