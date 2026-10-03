# PackInspect

本机包环境扫描工具。检测多种包管理器、列出已安装包、统计缓存占用、读写镜像源配置、
导出报告，并提供**安全的缓存清理**。技术栈：**Vue 3 + TypeScript + Vite** 前端，
**Tauri v2 (Rust)** 桌面外壳，自绘无边框窗口。

---

## 1. 一键启动

双击仓库根目录的 **`启动 PackInspect.cmd`** 即可。它会：

1. 缺 `node_modules` 时先跑 `npm install`
2. 检查 1420 端口；没起就最小化窗口启动 Vite devServer
3. 轮询端口直到可连接（每 5 秒打一个点，不会看起来像卡死）
4. 调用 `scripts\build.ps1 run` 编译并打开桌面应用

关闭应用窗口后，devServer 仍在最小化窗口里运行；要停它就关掉那个窗口。

启动逻辑在 **`scripts/launch.ps1`** 里，`.cmd` 只是一个极简外壳：

```cmd
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\launch.ps1"
```

> ⚠️ **批处理文件的硬约束（踩过两次的坑）**
>
> `cmd.exe` 是**按字节**读取 `.cmd` / `.bat` 的，因此这类文件必须同时满足：
> 1. **行尾是 CRLF**。若被编辑器改成 LF-only，脚本会被拆成
>    `'M' is not recognized` / `'ho' is not recognized` 之类的碎片错误 ——
>    现象就是「双击没反应 / 启动不了」。
> 2. **纯 ASCII**。按控制台 OEM 代码页解析，中文注释会破坏行结构。
>
> 因此仓库里加了 `.gitattributes` 强制 `*.cmd`/`*.bat`/`*.ps1` 使用 CRLF，
> 并且**所有启动逻辑都放在 `scripts/*.ps1`**，只让 `.cmd` 承担一行转发。
> 如果你改动了 `.cmd`，请确认行尾仍是 CRLF（`Get-Content` 看不出，用
> `Format-Hex` 或统计 `LF-only` 字节数）。

### 启动不了？先跑体检

```powershell
./scripts/build.ps1 doctor
```

它会逐项检查 Node.js / cargo / Visual Studio C++ 工具链 / WebView2 运行时，
缺什么直接给出安装地址。

### 常见问题

| 现象 | 原因与处理 |
|---|---|
| 窗口控制台停在 `[3/3] 编译并启动` 很久 | **正常**。首次要编译 400+ 个 crate，3~8 分钟。日志实时写在 `.logs\build.log` |
| 提示 `devServer 60 秒内未就绪` | 端口 1420 被占用。看 `.logs\vite-dev.log`，或先关掉占用的程序 |
| 双击 `.cmd` 弹出一堆 `'M' is not recognized` | `.cmd` 行尾被改成了 LF。用 `git checkout -- "启动 PackInspect.cmd"` 恢复（`.gitattributes` 会给出 CRLF） |
| 提示 `未找到 Node.js / cargo` | 跑 `doctor` 按提示安装，装完**重开终端**（PATH 需要刷新） |
| 编译报 `link.exe not found` | 缺 MSVC 生成工具。装「使用 C++ 的桌面开发」工作负载 |
| 窗口一片空白 | 多半是 WebView2 缺失，装 https://developer.microsoft.com/microsoft-edge/webview2/ |
| 界面中英混杂 / 主题不对 | 清掉 WebView 存储 `%LOCALAPPDATA%\dev.packinspect.app` 与 `%APPDATA%\PackInspect` 后重启 |

> ⚠️ **已知坑：Vite 的 IPv4/IPv6 绑定。** Vite 默认监听 `localhost`，在部分 Windows 上
> 只解析到 IPv6 `::1`。此时外部探测 `127.0.0.1:1420` 会被拒绝，启动脚本会一直空等到超时
> —— 看起来就像「启动器卡死」。本项目已在 `vite.config.ts` 里显式 `host: '127.0.0.1'`，
> 并把 `tauri.conf.json` 的 `devUrl` 改为 `http://127.0.0.1:1420`，同时让 `launch.ps1`
> 的探活按 `::1` → `127.0.0.1` → HTTP 依次回退，三重兜住这个问题。
>
> ⚠️ **另一个坑：日志文件位置。** cargo 的输出日志若放在项目根目录，Vite 的文件监听器
> 会在构建期间尝试 watch 这个被独占的文件，直接抛 `EBUSY` 崩溃。所以日志统一放在
> `.logs/`，并在 `vite.config.ts` 的 `watch.ignored` 里排除。

### 手动启动（开发时更常用）

```cmd
:: 1. 起前端 devServer（固定 1420，日志 .logs\vite-dev.log）
tauri-vite.cmd

:: 2. 编译并启动应用（日志 .logs\build.log）
scripts\build.ps1 run
```

> **为什么不用 `npm run tauri:dev`**：Tauri CLI 会执行 `tauri.conf.json` 里的
> `build.beforeDevCommand`（`npm run dev`），该命令依赖 `npm` 垫片，在非交互 shell
> 或 PATH 不含 npm 时会直接失败并中断构建。直接 `cargo run` 等效且少一层依赖 ——
> Tauri 二进制会自己读取 `build.devUrl` 连接已运行的 Vite。

## 打包与运行

### 产出可直接运行的单文件 exe

```
./scripts/build.ps1 package
```

产物（**双击即可运行，无需安装**）：

```
src-tauri/target/release/bundle/portable/PackInspect/
├── PackInspect.exe        约 3.9 MB
└── scripts/
    └── run-install.ps1    可见命令行窗口的运行时依赖
```

`bundle.targets` 设为 `["app"]`，因此**不会**生成安装向导、不写注册表、不留卸载项。
整个目录可以直接拷走使用。

> **只拷 `PackInspect.exe` 也能用**，但「执行安装」的可见命令行窗口需要同目录下的
> `scripts/run-install.ps1`。缺了它时该操作会给出一条明确说明打包缺文件的错误，
> 而不是静默失效 —— 见 `package_ops::run` 里对 `run_visible` 错误的处理。

### 为什么不用安装程序

早先配过 NSIS 向导（欢迎页 / 选目录 / 进度 / 完成 + 维护页），功能正常且逐页实测过，
但用户要的是「打包后直接运行」。单文件 exe 没有安装步骤，也没有"装到哪去了"的疑问。
如果以后需要分发安装包，把 `bundle.targets` 改回 `["nsis"]` 并补回
`bundle.windows.nsis` 段即可（注意字段名是 Tauri **v2** 的，
`oneClick` / `allowToChangeInstallationDirectory` 那类 v1 字段会导致构建失败）。

### 窗口外观

`decorations: false` + `shadow: true`：自绘标题栏，同时保留 Windows 给无边框窗口加的
标准 DWM 投影。

关于 `shadow` 的取舍（两种都试过）：

| 取值 | 效果 |
|---|---|
| `true` | 标准窗口投影，与其它桌面应用一致（**当前采用**） |
| `false` | 完全没有投影，窗口边缘是硬切的黑边，观感偏"残缺" |

Tauri 只提供开/关两态，**没有"轻一点"的档位**：它直接映射到 DWM 的投影，
强度由系统决定（Windows 11 下约为 `36px` 模糊、`0.3` 不透明度）。
若确实想自定义强度，只能走 `transparent: true` + CSS `box-shadow` ——
代价是失去 DWM 的原生圆角与投影，且透明窗口在 Windows 上有已知渲染问题，不建议。

`windowEffects`（mica / acrylic / blur）是另一类效果，**需要透明窗口**，
不是"更轻的投影"，不要用它来替代 `shadow`。

> 验证教训：**不要靠屏幕截图判断窗口阴影**。本项目的调试环境里前台窗口会覆盖采样点，
> 窗口超出屏幕时 `CopyFromScreen` 还会用白色填充，两次三番给出错误结论。
> 要判断投影应查 DWM 属性（`DwmGetWindowAttribute` 的 `DWMWA_EXTENDED_FRAME_BOUNDS`），
> 或直接交给用户确认。

### 打包时容易踩的坑

| 坑 | 症状 | 防回退 |
|---|---|---|
| `tauri.conf.json` 被写入 UTF-8 **BOM** | 构建报 `expected value at line 1 column 1`，看起来像文件为空 | `verify-config.mjs` 扫描所有 JSON 的 BOM |
| 把 Tauri **v1** 的 NSIS 字段名写进 v2 配置 | 构建报 `is not valid under any of the schemas` | `verify-config.mjs` 校验 nsis 字段是否都在 v2 白名单内（仅当配了 `bundle.windows.nsis` 时） |
| 用 PowerShell 改 `tauri.conf.json` | 同上（`Set-Content -Encoding utf8` 会加 BOM） | 同上 |
| 图标文件缺失 | 打包失败或退回默认图标 | `verify-config.mjs` 断言 bundle.icon 与 installerIcon 都存在 |

### 图标

`scripts/make-icon.py` 生成（需要 Pillow）：深色底 + 等距包裹箱 + 放大镜，
输出 32 / 128 / 256 的 PNG 与含 7 种尺寸的 `icon.ico`（16/24/32/48/64/128/256）。
不用官方 logo 是因为那涉及商标，且复杂路径在小尺寸下糊成一团。

### 前端逻辑回归校验

有两处前端逻辑用真实数据才能发现、又不需要浏览器即可验证，因此做成 Node 脚本并由
`./scripts/build.ps1 verify` 统一执行：

| 脚本 | 校验内容 |
|---|---|
| `verify-scan-merge.mjs` | **单管理器扫描不得清空其它管理器的数据**（曾导致「打开一个包管理器后，别的包里搜不到东西」） |
| `verify-search-parity.mjs` | **包列表分页与顶部搜索必须用同一套匹配规则**（曾导致「顶部搜得到、分页搜不到」） |

它们都直接读源码断言修复存在，再用模拟/真实数据跑对照，因此改回旧写法会立刻失败。

### 构建脚本

`scripts/build.ps1` 会自行完成环境预检与工具链注入（见 `scripts/env-preflight.ps1`，
其中 vcvars 的 PATH 采用**合并**而非替换，否则会丢掉 nodejs/python 目录）：

```powershell
./scripts/build.ps1 doctor     # 环境体检（缺什么、怎么装）
./scripts/build.ps1 check      # cargo check --all-targets（最快，不链接）
./scripts/build.ps1 test       # cargo test --lib
./scripts/build.ps1 clippy     # cargo clippy --all-targets
./scripts/build.ps1 build      # 只编译，不启动
./scripts/build.ps1 run        # 编译并启动桌面应用
./scripts/build.ps1 test -- <用例名> --nocapture   # `--` 之后原样透传给 cargo
```

cargo 的完整输出写入 `.logs/build.log`，控制台只显示编译步数、`Finished` 行与 error 行。
（不用 `2>&1 | Tee-Object` 的原因：cargo 把进度写到 stderr，PowerShell 会把它包装成
红色 `NativeCommandError`，看起来像失败，实际退出码为 0。）

### 功能自检

应用启动后，在 WebView 控制台执行（需要开启 devtools）可跑一遍核心链路：

```js
await import('/scripts/smoke.ts')
```

它会真实调用 IPC，验证探测 / 扫描 / 管理动作 / 包内子节点 / 图标 / 设置往返 /
链接白名单拦截，并把结果打印出来。

---

## 2. 界面结构

```
自绘标题栏（品牌 · 全局搜索 · 窗口控制）
├── 侧边栏（上下两区）
│   ├── 上区「包管理器」：品牌 logo + 名称 + 版本，已安装在前
│   │   └── 点击 → 进入该管理器的详情页
│   └── 下区「工具」：缓存占用 / 镜像源 / 设置
└── 主区
    ├── 工具栏：返回 / 当前范围 / 扫描 / 重新探测 / 清理缓存 / 统计体积
    ├── 提示区：错误条、操作结果、探测警告
    └── 视图
状态栏：主机 · 扫描时间 · 包数 · 缓存 · 扫描进度条 · 失败项 · 当前范围
```

### 全局搜索（标题栏）

同时命中两类目标，结果分组展示：

- **包管理器**（名称 / id / 生态）→ 进入该管理器详情页
- **已安装包**（名称 / 版本 / **显示名** / 路径）→ 进入其管理器的「包列表」分页

匹配会**忽略空格、连字符与点**，因此 `oh my posh` / `ohmyposh` / `Oh-My-Posh` 都能命中
winget 的 `JanDeDobbeleer.OhMyPosh`。把 `description` 纳入匹配很关键：
winget、cargo、dotnet 等生态把用户可读的名称放在这里，而 `name` 是机器 ID。

支持 `↑` `↓` 选择（自动滚进可视区，不会选中了却看不到）、`Enter` 跳转、`Esc` 关闭；
结果上限 50 条。

### 视图与分页

| 视图 | 说明 |
|---|---|
| **包管理**（默认落地页） | 一个管理器一张卡片：已检测到显示 logo / 版本 / 全局目录 / 缓存目录 / 包数量；未检测到显示「未检测到」+「前往官网下载」 |
| **管理器详情** | 点侧栏或卡片进入。头部是 logo + 版本 + 可执行路径，下面四个分页 |
| ⤷ 概览 | 包数量 / 缓存占用 / 生态 + 路径明细 + 快捷入口 |
| ⤷ 包列表 | 该管理器的已安装包（**只显示包名 + 版本**），右键打开管理菜单 |
| ⤷ 浏览 / 安装 | 搜索官方仓库里的新包；**已在本机安装的会标绿并提示**。点结果生成安装方案后，可**直接执行安装**，也可复制命令自行运行 |
| ⤷ 管理操作 | 镜像源入口、文档、下载引导 |
| 缓存占用 | 各管理器缓存目录的体积分布（一级子目录占用条） |
| 镜像源 | **左侧列出全部管理器**，右侧显示选中项的配置与「当前生效源」 |
| 设置 | 语言（中/英）、主题、启动扫描 + 运行环境 + 安全模型 + **关于** |

### 在线浏览能力（全部实跑验证过）

| 生态 | 端点 | 关键词搜索 | 备注 |
|---|---|---|---|
| npm / pnpm / yarn | registry.npmjs.org/-/v1/search | ✅ | 展示月下载量 |
| cargo | crates.io/api/v1/crates | ✅ | |
| dotnet | azuresearch-usnc.nuget.org/query | ✅ | |
| composer | packagist.org/search.json | ✅ | |
| gem | rubygems.org/api/v1/search.json | ✅ | |
| dart | pub.dev/api/search | ✅ | 列表接口只有包名，无版本 |
| powershellget | PowerShell Gallery OData (Atom XML) | ✅ | 版本取 NormalizedVersion（Version 是 NuGet 范围格式） |
| winget | 本地 `winget search` | ✅ | 已过滤 msstore 的 Store ID 条目 |
| pip | pypi.org/pypi/&lt;name&gt;/json | ❌ **精确名查询** | PyPI 搜索页由 JS 渲染、无公开 JSON 搜索 API |
| conda / maven / chocolatey / scoop / luarocks / cpan | — | — | 尚未接入，界面会明确说明而不是显示空结果 |

后端返回 `BrowseResult` 而不是裸数组，用来区分三种情况，界面据此给出不同提示：
**真的没有匹配** / **网络失败（附原因）** / **该生态不支持关键词搜索**。
一律显示「没有找到匹配的包」是最误导用户的做法。

搜索框旁边的提示条会说明当前生态的限制（例如 pip 需输入完整包名）。

**回归验证**：`cargo test --lib browse_ecosystems_online -- --ignored --nocapture`
会对上表 9 个生态各实搜一次并打印首条结果。上游改字段名这类问题只有实跑才能发现 ——
本轮修的 4 个 bug 里有 3 个正是如此。

### 渐进式扫描

扫描不是「一次全量、等最慢的那个」，而是**逐个管理器推进**：

1. 前端依次调用 `scan_manager`（一个管理器一次）
2. 每完成一个就把结果并进报告并立即渲染 —— 界面上能马上看到已扫到的包
3. 底部状态栏显示进度条与「已完成 / 总数 · 当前管理器」
4. 失败的管理器（未安装、命令超时）在状态栏单独列出，不会静默吞掉

这样 winget 要几秒、pip 要几秒、cargo 可能要十几秒时，用户不必盯着空白页干等。

### 关于图标

包管理器使用**矢量品牌 logo**（`src-tauri/src/icons.rs` 里用 SVG 路径重绘的形状 +
官方品牌色），深浅主题各一套配色，完全离线、无外部请求。
单个**包不再显示图标** —— 图标表达的是「它来自哪个包管理器」，
这一点已在侧边栏、卡片与详情页头部体现；包行上重复同一个图标只是噪音。

### 国际化

文案在 `src/i18n/locales/zh-CN.json` 与 `en-US.json`（各 287 条），与代码分离。
`index.ts` 在模块加载时校验两种语言的键完全一致，缺翻译会立刻抛错而不是显示键名。
新增语言：加一个 JSON 文件并登记到 `DICTS`。

---

## 3. 安全模型（最重要的一节）

| 约束 | 实现位置 |
|---|---|
| 前端不执行 shell、不读文件系统 | 不启用 Tauri shell 插件；`capabilities/default.json` 只授予 `core:window` 与 dialog 权限 |
| 命令必须走白名单 | `whitelist.rs` → `MANAGERS[].ops` 定义每个 (manager, op) 的**静态参数数组** |
| 前端不能传命令行 | IPC 只接收 `manager id` / `op` / 绝对路径；Rust 侧拼装参数 |
| 动态值必须校验 | `validate.rs`：包名、URL、配置键、路径逐项校验 |
| 清理不能越界 | `validate::ensure_within()` 保证目标严格位于缓存根之内，拒绝 `..`、软链接逃逸、关键系统路径 |
| 更新/卸载/安装受控 | 参数来自 `whitelist::op_args` 的**静态数组模板**；操作名收敛为 `PackageOp` 枚举；`confirm` 必须为 true；带超时；不经 shell（见 §3.1） |
| 清理必须二次确认 | `CleanDialog.vue` 三段式：勾选 → dry-run 预览 → 二次确认（≥1GB 需手打 `DELETE`） |
| 写配置前备份 | `registry::write()` 先写 `*.bak-<时间戳>`，再原子替换（写 `.tmp` + rename） |
| 打开链接受控 | `settings::check_url()`：仅 https + 域名白名单；`kind = manager` 时由后端取官网地址，前端无法自带 URL |
| 不联网抓图标 | 图标由后端按包名哈希生成内联 SVG（`icons.rs`），离线可用、无 CSP 冲突 |

### 3.1 更新 / 卸载 / 安装：真实可执行

**两个入口，同一套约束。** 唯一会改动环境的命令是 
un_package_op：

| 入口 | 动作 |
|---|---|
| 右键已安装的包 | 更新 / 卸载 / 重新安装 |
| 「浏览 / 安装」分页的结果 | 安装新包（也可只复制命令） |

两条入口都先弹确认对话框、显示将运行的确切命令，再走同一个后端命令。
**卸载**还会额外要求勾选「我已了解后果」（它会真的删掉东西）；
安装与更新不勾选，因为不涉及数据丢失，但依然有明确的一次确认。

**执行时会打开一个命令行窗口**，你可以实时看到下载与安装进度 ——
只给一个转圈图标然后突然弹结果的话，卡住时完全无法判断发生了什么。
窗口在结束后自动关闭，而完整输出同时写入
%APPDATA%\PackInspect\logs\<管理器>-<操作>-<包名>.log，
结果对话框会显示该路径，事后仍可回看。

实现要点（src-tauri/src/console.rs + scripts/run-install.ps1）：
子进程一旦拥有真实控制台，输出就直接给用户、父进程拿不到，因此走
「可见控制台 + 包装脚本落盘日志」的折中。退出码通过独立的纯 ASCII
<log>.status 文件回传（不与日志混编）。包装脚本刻意只用 Windows PowerShell
5.1 也支持的语法，参数以 JSON 传入 —— 这几条都是踩坑后固化的约束，
console.rs 里有对应测试直接断言脚本不得退化。

约束逐条如下：

这三项**会改动你的真实环境**，因此是本项目安全约束最密集的地方，逐条如下：

| # | 约束 | 实现位置 |
|---|---|---|
| 1 | 操作名只能是 `update` / `uninstall` / `install` | `validate::PackageOp`（枚举，`parse` 拒绝其它值） |
| 2 | 包管理器必须已知、包名必须合法 | `validate::resolve_package_op` → `validate_manager` + `package_name` |
| 3 | 参数只能来自静态模板 | `whitelist::op_args`；`package_ops::render_args` 断言模板**恰好含一个 `{}`**，只替换它 |
| 4 | 必须经过二次确认 | 前端 `PackageOpDialog`（破坏性操作还要勾选「我已了解后果」）+ 后端校验 `confirm == true` |
| 5 | 不能无限等待 | 卸载 120s、安装/更新 300s 强制超时；超时会明确回显「可能仍在后台进行」 |
| 6 | 不经过 shell | `executor` 统一处理 Windows 的 `.cmd` 包装；包名已排除 `;` `|` `$` 反引号、换行等一切元字符 |

执行后会自动重扫该管理器，保证列表与磁盘一致；结果完整回显 stdout / stderr 与退出码，
失败时你可以复制对话框里给出的命令自行重跑。

不支持的操作会明确说明而不是假装可用：例如 NuGet 的全局包目录没有官方卸载子命令，
所以 dotnet 的「卸载」显示为不可用并给出替代做法。

**关于更新 / 卸载 / 安装**：这三个动作会改动用户的真实环境，且包管理器的交互提示
（确认、依赖冲突、权限）无法在后台管道里可靠完成。因此一期它们**只是占位按钮**：
右键菜单里显示为灰色 + `占位` 标签，鼠标悬停会给出**等价的官方命令**供用户自行执行。
「管理此包」「查看安装详情」「打开包主页」则是真正可用的。

---

## 4. 目录结构

```
packInspect/
├─ 启动 PackInspect.cmd          # 一键启动
├─ tauri-vite.cmd / tauri-run.cmd / tauri-dev.cmd
├─ scripts/
│  ├─ build.ps1                  # 构建/测试入口（自动注入 cargo + MSVC 环境）
│  └─ smoke.ts                   # 功能自检脚本（WebView 控制台里跑）
├─ src/                          # 前端（WebView 内运行，无系统权限）
│  ├─ main.ts / App.vue
│  ├─ api/index.ts               # 唯一 IPC 通道
│  ├─ types/index.ts             # 与 Rust models.rs 严格镜像
│  ├─ i18n/index.ts              # 中英文案 + t() 插值
│  ├─ stores/app.ts              # 扫描结果、右键菜单缓存、清理状态机
│  ├─ stores/settings.ts         # 语言 / 主题 / 启动行为
│  ├─ styles/theme.css           # 深色 + 浅色主题变量
│  ├─ components/                # TitleBar / NavSidebar / StatusBar / GlobalBanner
│  │                             # PackageContextMenu / PackageDetailDrawer / CleanDialog
│  └─ views/                     # ManagerView / PackagesView / CacheView
│                                # RegistryView / SettingsView / ReportView
└─ src-tauri/                    # 后端（唯一有系统权限的一侧）
   ├─ Cargo.toml / build.rs / tauri.conf.json / capabilities/default.json
   └─ src/
      ├─ main.rs / lib.rs        # 入口 + command 注册
      ├─ commands.rs             # Tauri command 层（spawn_blocking 包装）
      ├─ report.rs               # 扫描编排 + 报告导出
      ├─ manager.rs              # 探测：可执行文件/版本/全局根/缓存目录
      ├─ packages.rs             # 各生态的已安装包枚举
      ├─ actions.rs              # 右键管理动作 + 包主页 URL
      ├─ plugins.rs              # 包内子节点（插件/依赖/文件）
      ├─ icons.rs                # 内联 SVG 图标生成 + 缓存
      ├─ settings.rs             # 设置持久化 + 受控打开链接
      ├─ cleaner.rs              # 清理候选枚举 + 安全删除
      ├─ registry.rs             # 镜像源读取/解析/备份写回
      ├─ executor.rs             # 命令解析与超时执行（唯一 spawn 点）
      ├─ whitelist.rs            # 包管理器定义 + 命令白名单
      ├─ validate.rs             # 输入校验 + 路径守卫
      ├─ fsutil.rs               # 目录体积统计 / 文件读写 / 格式化
      ├─ models.rs               # 前后端共享数据结构
      └─ error.rs                # 结构化错误（带 code）
```

依赖方向自上而下，下层不反向依赖上层：

```
commands → report · actions · plugins · settings
         → manager · packages · cleaner · registry
         → executor · validate · whitelist · fsutil · icons → models · error
```

---

## 5. 环境要求

- **Node.js** ≥ 18（实测 24.16.0）
- **Rust** ≥ 1.77.2 + `cargo`（实测 1.99.0）
- **Windows**：需 [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)。
  `scripts/build.ps1` 会自动用 `vswhere` 定位并注入环境；命令行手动调用 cargo 时
  需先跑 `vcvars64.bat` 或在「Developer PowerShell for VS」里操作。
- **WebView2**：Windows 10/11 通常已内置

### 网络受限时

```powershell
# 本地代理（如 Clash 监听 7890）—— build.ps1 已默认设置
$env:HTTP_PROXY  = 'http://127.0.0.1:7890'
$env:HTTPS_PROXY = 'http://127.0.0.1:7890'

# 或字节 rsproxy 镜像
$env:RUSTUP_DIST_SERVER = 'https://rsproxy.cn'
$env:RUSTUP_UPDATE_ROOT = 'https://rsproxy.cn/rustup'
```

`rustup toolchain install` 在慢速网络下可能长时间停在 `.partial` 文件上；
加代理后重跑会从中断处恢复。

---

## 6. 支持的包管理器

共 **39 个**，按平台与生态族分组。每个都带 `platforms` 标记，**不属于当前系统的
不会出现在侧边栏**（可在设置里打开「显示其它平台的包管理器」查看）。

### 平台适用性

| 平台标记 | 含义 | 例子 |
|---|---|---|
| `all` | 全平台 | npm、pip、cargo、bun、opam、vcpkg |
| `win` | 仅 Windows | winget、chocolatey、scoop |
| `linux` | 仅 Linux | apt、pacman、dnf、flatpak、snap |
| `macos` | 仅 macOS | CocoaPods（需要 Xcode） |
| `unix` | macOS / Linux | Homebrew（Windows 上只能在 WSL 里用） |

**本机（Windows）实测**：39 个中 32 个适用、7 个不适用，检测到 7 个
（npm / pip / cargo / dotnet / winget / powershellget / maven）。
平台不适用的管理器会显示「本系统不适用」并说明它实际支持哪些平台，
而不是谎报「未在 PATH 中找到」—— 后者会让你以为是自己环境有问题。

### 一期（已实测扫描）

| id | 语言 | 全局包识别方式 | 缓存目录 | 镜像源 |
|---|---|---|---|---|
| `npm` | Node.js | `npm root -g` + 扫 package.json，兜底 `ls -g --json` | `%LOCALAPPDATA%\npm-cache` | ✅ |
| `pnpm` | Node.js | `pnpm root -g` | `%LOCALAPPDATA%\pnpm\store` | ✅ |
| `yarn` | Node.js | `yarn global dir` | `%LOCALAPPDATA%\Yarn\Cache` | ✅ |
| `pip` | Python | `pip list --format=json` + dist-info 定位 | `%LOCALAPPDATA%\pip\Cache` | ✅ |
| `cargo` | Rust | `cargo install --list` | `~/.cargo/registry` | ✅ |
| `dotnet` | .NET | 扫 `~/.nuget/packages/<Id>/<Version>` | `%LOCALAPPDATA%\NuGet\v3-cache` | — |
| `winget` | Windows | `winget list`（按表头列位切分定宽表格） | `%LOCALAPPDATA%\Microsoft\WinGet` | — |

### 二 / 三期（扫描已接入，未在真机逐一验证）

`powershellget`（`Get-Module -ListAvailable`）、`composer`、`gem`、`go`、`maven`、
`chocolatey`、`scoop`、`conda`、`dart`(pub)、`luarocks`、`cpan`

### 四期：语言生态与构建工具

| id | 语言 | 能否列出已安装 | 说明 |
|---|---|---|---|
| `bun` | Node.js | ✅ `pm ls -g` | 该命令官方文档**没写**，来自上游源码；失败时退回读全局 package.json |
| `deno` | TypeScript | ⚠️ **枚举目录** | deno **没有**列出全局包的命令，只能枚举其 bin 目录 |
| `julia` | Julia | ✅ `Pkg.status()` | 列的是**当前活动环境**，不是跨环境全局集合 |
| `mix` | Elixir | ❌ 无 | Elixir 依赖是项目级的，没有全局已安装列表 |
| `gradle` | Java | ❌ 无 | 且每次调用都执行项目构建脚本（任意代码执行），因此**未开放任何操作** |
| `vcpkg` | C++ | ✅ `list` | classic 模式下该已安装树被**所有使用它的项目共享** |
| `conan` | C++ | ✅ `list` | 卸载走 `remove {}`；包名经严格校验以防 `*` 清空整个缓存 |
| `swift` | Swift | ❌ 无 | SwiftPM 依赖是项目级的 |
| `cocoapods` | Ruby | ❌ 无 | 仅 macOS；`pod list` 列的是**可用** pod 目录而非已安装 |

### 五期：Linux 发行版（仅 Linux）

`apt`（用 `apt-get`）、`pacman`、`dnf`（`yum` 是其兼容层，合并为一个条目）、
`flatpak`、`snap`、`pipx`（唯一跨平台的一个）

### 六期：语言工具链

| id | 语言 | 说明 |
|---|---|---|
| `opam` | OCaml | 列出的是**当前 switch** 的包；opam 没有跨 switch 列表 |
| `dub` | D | **没有 install 命令**；`dub remove` 只删缓存，且无法用 CLI 移除项目依赖 |
| `nimble` | Nim | 不带 `--ver` 时只有包名没有版本 |
| `cabal` | Haskell | **没有 uninstall 命令**；`list --installed` 读的是 GHC 包库而非 store |
| `stack` | Haskell | **没有全局列表**；`stack uninstall` 是只打印建议的空操作 |

### 能力边界是刻意保留的

多个生态**确实没有**某些能力，工具不会编命令来凑齐按钮：

- **deno**：没有列出全局包的命令（`deno info` 只打印缓存路径，`deno list`
  只列项目依赖）。因此改为枚举其全局 bin 目录（`$DENO_INSTALL_ROOT` 或 `~/.deno/bin`），
  并明确说明这是按已安装的可执行文件统计的。
- **dotnet**：全局包目录没有官方卸载子命令，因此「卸载」不可用。
- **gradle**：每次调用都执行项目构建脚本，静态参数白名单约束不了，因此只开放版本探测。
- **mix / swift / cocoapods**：依赖是项目级的，没有全局列表，对应操作不可用。

### 安全上有意排除的参数

核实过程中确认了一批**危险参数**，它们不会出现在白名单里：

| 生态 | 排除的参数 | 原因 |
|---|---|---|
| apt | `autoremove` | 会移除"不再被需要"的自动依赖；apt 自己的 man 页都提醒先检查列表 |
| apt | `dist-upgrade` | 可能移除已安装的包 |
| pacman | `-Sy`（单独用） | 部分升级，官方明令禁止 |
| pacman | `-Rdd` / `-Rc` | 跳过依赖检查 / 级联移除依赖它的包 |
| dnf | `autoremove`、不带 `--noautoremove` 的 remove | 会连带移除因此变得不再需要的依赖 |
| flatpak | `--unused` / `--all` / `--delete-data` | 移除未被需要的运行时 / 不可逆的数据删除 |
| snap | `--purge` | 跳过快照，移除不可恢复 |
| brew | `--zap` / `--force` / `--ignore-dependencies` | 删共享文件 / 该包全部版本 / 别的包正依赖的东西 |
| vcpkg | `--recurse` | 允许移除命令行未点名的包 |
| conan | 包名含 `*` | `conan remove "*"` 会清空整个本地缓存 —— 由严格包名校验挡住 |
| cabal / gradle / stack | store / 构建脚本 / `stack upgrade` | 内容寻址共享 store / 任意代码 / 替换二进制本身 |

**包名校验**是这些防线的主要手段：`*` `?` `[` 等通配与正则元字符被字符集排除，
因此 `apt-get` 的正则回退与 `dnf` 的 glob 展开都无法被触发；系统级生态另有一套
更严格的字符集。前导 `-` 也被拒绝，防止选项注入（如 `pip uninstall -y --target=...`）。

> 逐个管理器的**核实记录与证据强度**（实测 / 文档 / 源码 / 存疑）见
> [docs/manager-verification.md](docs/manager-verification.md)。
> 重要提醒：除 npm 外，这些命令**没有在本机实际执行过**，依据是官方文档与上游源码。

新增一个管理器：在 `whitelist.rs` 的 `MANAGERS` 加一条定义（含 `platforms`）→
在 `packages.rs` 补该生态的枚举函数 → 在 `report.rs` 的 `collect_packages` 加一个分支 →
在 `icons.rs` 加品牌色与矢量标记。前端无需改动（侧边栏与表格完全数据驱动）。

---

## 7. 后端命令清单

| command | 作用 | 写操作 |
|---|---|---|
| `supported_managers` | 静态列出支持的管理器与允许的操作 | 否 |
| `detect_managers` | 探测可执行文件/版本/全局目录/缓存目录 | 否（缓存 5 分钟） |
| `install_hints` | 未检测到的管理器 + 官方下载入口 | 否 |
| `get_registry` / `get_all_registries` | 读取镜像源配置 | 否 |
| `preview_registry_change` | 预览保存后的配置文件内容 | 否 |
| `set_registry` | 写回镜像源（先备份） | **是** |
| `run_scan` | 一次性全量扫描，返回完整报告 | 否 |
| `scan_manager` | **单管理器扫描**（渐进式扫描的基础），失败返回 ok=false + reason | 否 |
| `get_cache_stats` | 单管理器缓存占用 + 一级子目录分布 | 否 |
| `package_icon` | 取包图标（后端缓存） | 否 |
| `package_actions` | 取右键管理动作（含占位标记与等价命令） | 否 |
| `package_plugins` | 展开包内子节点 | 否 |
| `run_package_op` | **执行真实更新/卸载/安装**；confirm 必须为 true | **是** |
| `list_clean_candidates` | 枚举清理候选 | 否 |
| `clean_caches` | 清理；`dryRun` 默认 `true` | **是**（`dryRun=false` 时） |
| `export_report` | 导出 JSON / CSV / Markdown | **是**（写新文件） |
| `open_external_link` | 用系统浏览器打开（https + 域名白名单） | 否 |
| `get_settings` / `save_settings` | 读写设置 | **是**（保存时） |
| `get_diagnostics` / `parent_dir` | 环境信息 / 取父目录 | 否 |

---

## 8. 测试

```bash
scripts\build.ps1 test          # Rust：54 个用例
npm run typecheck               # 前端类型与数据契约校验
```

安全相关用例集中在 `validate.rs`（注入攻击、路径逃逸）、`executor.rs`
（PATH 解析回退、超时不失控）、`whitelist.rs`（未知操作被拒、一期覆盖完整）、
`cleaner.rs`（禁止目录名、候选 id 稳定、dry-run 无副作用）、`actions.rs`
（破坏性动作必须禁用、包主页 URL 只指向已知仓库）、`settings.rs`
（链接白名单、设置合法性）。

### 验证状态

| 项目 | 命令 | 结果 |
|---|---|---|
| Rust 编译（含测试目标） | `cargo check --all-targets` | ✅ 0 告警 |
| Rust 单元测试 | `cargo test --lib` | ✅ 73 passed / 0 failed |
| 前端类型检查 | `vue-tsc --noEmit` | ✅ 通过 |
| 前端生产构建 | `vite build` | ✅ 通过 |
| 应用实际启动 | `build.ps1 run` | ✅ 窗口正常，探测到 7/18 管理器，扫描出 500+ 个包 / 1.9 GB 缓存 |
| 包列表 / 图标 / 右键菜单 | 截图人工核对 | ✅ 见 §2 |
| 设置页（语言 / 主题） | 截图人工核对 | ✅ 渲染正常 |

尚未验证：二期 / 三期管理器的真实扫描结果、浅色主题下的逐项视觉走查、
`npm run tauri:build` 打包安装程序。

---

## 9. 已知限制

- **项目局部包**扫描未实现：`scope` 目前只有 `global` 与 `system`。
  规划中的做法是在用户指定根目录内向上查找 `package-lock.json` / `pyproject.toml` 等标记文件。
- **「最新版本」比对未实现**（`latestVersion` 恒为 `null`），需要联网查询仓库元数据。
- 缓存体积统计对超大目录会触发 `MAX_SCAN_ENTRIES` 截断，此时 `truncated = true`。
- pnpm store、cargo registry、Go mod cache、NuGet 全局包目录为**内容寻址/共享**结构，
  删除会破坏已有项目，因此这些根目录本身不可一键删除，只允许清理其下的缓存子目录。
- `src-tauri/icons/` 是脚本生成的占位图标，正式发布前用 `npx tauri icon <1024px 图>` 替换。
- 本工具的**图标是本地生成的字母图标**，不是各包的真实 logo。这是刻意取舍：
  抓 favicon 需要给 WebView 放行任意域名，与安全约束冲突，且离线环境必然空白。