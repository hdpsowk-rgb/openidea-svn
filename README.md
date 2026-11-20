# SVN for OpenIDEA

在 OpenIDEA 内查看 SVN 变更、对比文件、勾选提交、更新和查看历史。标准 WebAssembly 插件，文件差异直接使用 IDE 自带查看器。

适配并实际验证 **OpenIDEA 0.1.47 / macOS**，要求插件 API 1.10。Windows 使用同一插件包，但尚未进行 Windows 真机验证。本插件调用本机 SVN，不附带 SVN 程序。

## 安装

1. 确认本机已有 `svn` 命令；Windows 需要命令行客户端 `svn.exe`。
2. 下载 [GitHub Releases](https://github.com/hdpsowk-rgb/openidea-svn/releases) 中的 `hdpsowk-rgb.svn-0.1.7.oiplugin`。
3. OpenIDEA → **Settings → Plugins → 齿轮 → Install Plugin from Package File…**，选择 `.oiplugin`，按 IDE 提示安装。
4. 允许插件运行 `svn`。如果没有自动找到它，在该插件的 Permissions → `process:svn` → **Choose executable…** 中选择实际可执行文件。macOS Homebrew 常见位置为 `/opt/homebrew/bin/svn`。
5. 打开已有 SVN 工作副本，用 **Find Action（⌘⇧A / Ctrl+Shift+A）** 搜索 **SVN: Open Tool Window**；也可以从 Tools 菜单打开。

ZIP 与 `.oiplugin` 内容相同。也可解压 ZIP，使用 **Install Plugin from Disk…** 选择包含 `plugin.toml` 的目录。普通 IntelliJ 插件包不适用于 OpenIDEA。

0.1.7 新增 40 种文件图标与 14 种目录图标，覆盖常用语言、配置、资源、测试和项目文件。图标随插件离线打包，不读取文件内容，不增加 SVN 请求；与 IDE 文件图标主题独立。保留目录双击、忽略/取消忽略和默认关闭的自动刷新。

## 日常操作

- **刷新设置**：工具栏下方可勾选「自动刷新」并设置 5–3600 秒间隔，默认关闭，默认间隔值为 30 秒。按工作区保存，重新打开后恢复；开启时仅在面板可见且没有进行中的操作时刷新。关闭时，保存文件和面板重新显示不会触发后台刷新；首次打开、切换目录、手动刷新及 SVN 操作完成后会读取一次状态。
- **工作目录**：顶部下拉框列出当前工作区的项目目录，例如 `frontend` / `backend`。切换后，变更、日志、更新和提交都作用于选中目录；记住最近选择，各目录保留当前面板的提交说明草稿。操作进行中禁止切换，切换时清空文件勾选，避免误提交。非 SVN 目录会显示提示，可继续切换。
- **本地变更**：显示修改、新增、删除、缺失、未跟踪、冲突和属性变化，默认按目录树显示，可切换为完整路径列表，并按工作区保存视图选择。支持双击目录展开/收起、展开/收起全部、方向键导航和路径筛选；筛选时自动展开匹配目录。目录复选框只勾选其下可提交的变更，部分勾选显示半选状态。刷新、折叠和切换视图保留文件勾选。项目树同步显示状态标记。
- **文件图标**：按精确文件名、测试文件约定和扩展名识别，特殊文件优先，例如 Composer、NPM、Docker、Git 配置和锁文件。目录按名称分类；普通目录区分展开/收起。图标使用本地 SVG，在树状、列表和浅色/深色界面中复用，匹配结果缓存最多 2048 项。
- **差异**：双击文件打开 IDE 原生左右对比，也可切换统一视图。主项目本地一侧包含编辑器尚未保存的内容；OpenIDEA 0.1.47 未向插件开放的附加目录显示已保存内容，有未保存修改时提示先保存。提交必须先保存。
- **提交**：勾选文件、填写说明、点击提交；原生确认框列出最终目标。只提交已选路径，不自动添加同目录的其他文件。提交成功后清空说明并刷新。
- **添加**：先点中未跟踪路径，再点添加。目录按空深度添加；刷新后可继续显式添加其子文件。
- **忽略 / 取消忽略**：点击一个未跟踪文件或目录，在「忽略…」中选择忽略，插件会追加父目录的 `svn:ignore` 规则并保留本地文件。开启「显示已忽略」，点中目标后选择取消忽略。同名规则直接删除；本目录的通配符规则会打开编辑框供明确修改，其他规则保留。父目录属性变化可随后提交。继承的 `svn:global-ignores` 和 SVN 客户端全局配置不会被自动改写，会给出提示。
- **删除 / 还原**：点中目标后操作，丢弃修改前有确认。点击目录行再还原会确认整个目录下的全部已跟踪修改；勾选文件后还原只处理选中路径。新增路径会取消版本控制。复制/替换目录暂不直接还原；阻塞项、已删除但在磁盘上仍有内容的路径也会拦截，避免 SVN 覆盖本地内容。无法确认附加目录中删除路径的磁盘状态时，需先备份并在 IDE 终端明确处理。删除目录会展开其后代供提交选择；提交目录删除必须同时勾选全部后代。
- **更新 / 清理**：作用于选中工作目录；更新忽略外部工作副本。存在未保存修改时会阻止更新。清理用于 SVN 管理锁，不清除未跟踪文件。
- **冲突**：点击冲突文件，在下拉框选择我的版本、仓库版本或标记已解决。
- **日志**：加载提交记录及改动路径，支持继续加载更早版本；双击文件打开该版本前后的原生差异。
- **操作输出**：更新、提交及文件写操作显示进度和输出，可从面板或 IDE 后台任务取消。取消之后仍需检查工作副本，已经完成的修改不会自动回滚。

项目树、编辑器和文件标签页提供 SVN 右键菜单，动作也可在 IDE 的 Keymap 中绑定快捷键。插件不预占 Git 快捷键。

## 认证

普通面板命令使用 SVN 已有缓存凭据，并设置 `--non-interactive`。认证失败时，点击 **认证**，插件会在 **IDE 内置终端**运行交互式 `svn info -r HEAD`；由 SVN 自身询问凭据并按其设置缓存。完成后回到面板重试。

插件不保存密码、不把密码放入命令参数，也不自动接受服务器证书。

## 边界

- 本次覆盖已有工作副本的日常操作；检出向导、仓库浏览、分支/合并向导和三方冲突编辑器不在首版范围。
- SVN 使用独立工具窗口；原生 Git 的 Commit/Git 窗口由 OpenIDEA 管理。
- 一次最多 200 个路径，提交说明最多 4096 UTF-8 字节；日志每页 50 条。
- 文本差异限制每侧 1 MiB、UTF-8；二进制/其他编码文件给出提示。目录没有文本内容，属性变更会标记，但尚无属性编辑面板。
- 结构化读取使用宿主有大小限制的一次性调用；读取期间使用 IDE 后台任务的取消入口。写操作使用流式进程句柄，面板中可取消。操作输出只保留有界内容。
- 嵌套独立工作副本及 SVN externals 请单独作为项目打开。项目位于工作副本子目录时，默认目标为当前项目，工作副本外的兄弟目录不混入提交。
- 替换节点（SVN 状态 `R`）可能隐含删除旧目录子树，首版阻止从面板直接提交。请先还原再修改，或在 IDE 终端检查清楚后显式提交。
- OpenIDEA 0.1.47 的附加目录 API 不完整：普通文件请从左侧项目树打开；面板差异、日志及 SVN 操作支持选中附加目录。兼容实现通过 SVN 绝对路径调用，不增加 shell、助手进程或额外权限。
- 远程开发主机与 Windows GUI 尚未验证，不声明已通过。

## 开发与构建

需要 Rust stable（edition 2024）、`wasm32-wasip2` 目标、Python 3.9+；Node.js 18+ 用于前端状态测试；`svnadmin` 用于本地仓库集成测试。

```sh
rustup target add wasm32-wasip2
cargo test --locked
npm test
python3 tools/build-file-icons.py --check
python3 tools/build.py
```

产物位于 `release/`，构建脚本核对固定 SDK 的 Git blob 校验值，执行锁定依赖的 release 构建，并检查安装包布局。OpenIDEA 实际安装时还会验证 manifest、资源、Wasm 组件接口和权限。

官方 SDK 源码固定在 `vendor/openidea-plugin`，来源为 OpenIDEA v0.1.47；完整提交与文件校验值见 `PROVENANCE.json`。源码无需再次克隆 SDK，首次 Cargo 构建仍需下载 crates.io 依赖。

卸载使用 IDE 插件页的 Uninstall。


## Permissions and privacy

`process:svn` is required to invoke the user-approved SVN executable for status, history, diffs and working-copy operations. Write operations use explicit paths and selected commits receive a confirmation dialog. The plugin does not execute a shell or download an SVN client. `project.read` and `editor.read` support local metadata, native diffs and unsaved-file checks; `storage` retains panel preferences and commit drafts; `events` observes local file changes. No network capability is requested by the plugin itself; repository connections are made by SVN using its existing configuration and credential cache. The plugin never stores passwords.

Community package ID: `hdpsowk-rgb.svn`. If upgrading from the development-only `local.svn` package, disable the old plugin to avoid duplicate SVN panels. The two IDs have separate settings and draft storage.

## English summary

Manage existing Subversion working copies inside OpenIDEA: select a workspace directory, browse local changes in a collapsible directory tree or a flat list, inspect native diffs, explicitly add/revert/resolve files, commit selected paths, update/clean up the working copy, and browse revision history. Automatic refresh is **off by default** and can be enabled for visible panels with a configurable interval. Requires an installed SVN command-line client. Tested on macOS with OpenIDEA 0.1.47/API 1.10; Windows path behavior has regression coverage, but Windows GUI and remote hosts remain unverified.
