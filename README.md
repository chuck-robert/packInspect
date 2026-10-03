# PackInspect

本机包环境扫描工具。扫描 pip / npm / pnpm / yarn / bun / uv / cargo / go / gem 的已安装包、
全局与项目局部包、缓存目录占用、镜像源配置；支持导出报告与**安全清理缓存**。

技术栈：**Vue 3 + TypeScript + Vite** 前端，**Tauri v2 (Rust)** 桌面外壳。

---

## 1. 安全模型（最重要的一节）

| 约束 | 实现位置 |
|---|---|
| 前端不执行 shell、不读文件系统 | 不启用 Tauri shell 插件；`src-tauri/capabilities/default.json` 只授予 `core:default` 与 dialog 权限 |
| 命令必须走白名单 | `src-tauri/src/whitelist.rs` → `MANAGERS[].ops` 定义每个 (manager, op) 的**静态参数数组** |
| 前端不能传命令行 | IPC 只接收 `manager id` / `op` / 绝对路径；Rust 侧拼装参数 |
| 动态值必须校验 | `src-tauri/src/validate.rs`：包名、URL、配置键、路径逐项校验 |
| 清理不能越界 | `validate::ensure_within()` 保证目标严格位于缓存根之内，且拒绝 `..`、软链接逃逸、关键系统路径 |
| 不卸载任何包 | 全项目无 `uninstall` 调用；清理白名单只有 cache / temp / oldversion（旧版本仅对全局 node 包） |
| 清理必须二次确认 | `CleanDialog.vue` 三段式：勾选 → dry-run 预览 → 二次确认（≥1GB 需手打 `DELETE`） |
| 写配置前备份 | `registry::write()` 先写 `*.bak-<时间戳>`，再原子替换（写 `.tmp` + rename） |

## 2. 目录结构

```
packInspect/
├─ index.html
├─ package.json / vite.config.ts / tsconfig.json
├─ src/                          # 前端（WebView 内运行，无系统权限）
│  ├─ main.ts / App.vue
│  ├─ api/index.ts               # Tauri IPC 封装（前端与系统的唯一通道）
│  ├─ types/index.ts             # 与 Rust models.rs 一一对应的类型镜像
│  ├─ stores/app.ts              # Pinia：扫描结果、清理状态机、日志
│  ├─ utils/format.ts            # 体积/时间/路径格式化
│  ├─ styles/theme.css           # 深色主题（变量集中在 :root）
│  ├─ components/                # NavSidebar / StatusBar / GlobalBanner / CleanDialog
│  └─ views/                     # PackagesView / CacheView / RegistryView / ReportView
└─ src-tauri/                    # 后端（唯一有系统权限的一侧）
   ├─ Cargo.toml / build.rs / tauri.conf.json
   ├─ capabilities/default.json  # 权限清单
   ├─ icons/                     # 占位图标（建议用 `npx tauri icon` 重新生成）
   └─ src/
      ├─ main.rs / lib.rs        # 入口 + command 注册
      ├─ commands.rs             # Tauri command 层（spawn_blocking 包装）
      ├─ report.rs               # 扫描编排 + 报告导出（JSON/CSV/Markdown）
      ├─ manager.rs              # 管理器探测：可执行文件/版本/全局根/缓存目录
      ├─ packages.rs             # 已安装包枚举 + 冗余旧版本识别
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
commands → report → manager · packages · cleaner · registry
                  → executor · validate · whitelist · fsutil → models · error
```

## 3. 环境要求

- **Node.js** ≥ 18（前端构建；实测 Node 24 通过）
- **Rust** ≥ 1.77.2 + `cargo`（实测 1.99.0 通过）
- **Windows**：需安装 [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（MSVC 链接器）。
  命令行调用 `cargo` 前需先执行 `vcvars64.bat`，或在「Developer PowerShell for VS」中操作。
- **WebView2**：Windows 10/11 通常已内置

### 网络受限时

`rustup` / `cargo` 首次拉取依赖较慢，可用镜像或本地代理：

```powershell
# 方式一：本地代理（如 Clash 监听 7890）
$env:HTTP_PROXY  = 'http://127.0.0.1:7890'
$env:HTTPS_PROXY = 'http://127.0.0.1:7890'

# 方式二：字节 rsproxy 镜像
$env:RUSTUP_DIST_SERVER = 'https://rsproxy.cn'
$env:RUSTUP_UPDATE_ROOT = 'https://rsproxy.cn/rustup'

# cargo 依赖走镜像（写入 ~/.cargo/config.toml）
# [source.crates-io]
# replace-with = 'rsproxy-sparse'
# [source.rsproxy-sparse]
# registry = "sparse+https://rsproxy.cn/index/"
```

`rustup toolchain install` 在慢速网络下可能长时间停在 `.partial` 文件上；加代理后重跑会自动从中断处恢复。

## 4. 开发

### 推荐：使用启动脚本（Windows，非交互环境友好）

仓库根目录提供三个 `.cmd` 启动器。它们会自行补齐 cargo 路径与 MSVC 环境，
不依赖当前 shell 的 PATH，因此可直接双击或从任意终端调用：

```cmd
:: 1. 先起前端 devServer（固定 1420，日志 vite-dev.log）
tauri-vite.cmd

:: 2. 再起桌面应用（编译 Rust 并打开窗口，日志 app-run.log）
tauri-run.cmd
```

> **为什么不用 `npm run tauri:dev`**：Tauri CLI 会执行 `tauri.conf.json` 里的
> `build.beforeDevCommand`（即 `npm run dev`），该命令依赖 `npm` 垫片；
> 在非交互 shell / 无 npm 的 PATH 下会直接失败并中断构建。
> `tauri-run.cmd` 改为直接 `cargo run`：Tauri 二进制本身会读取 `build.devUrl`
> 连接已运行的 Vite，效果等价且少一层依赖。
>
> 另有 `tauri-dev.cmd`（走 Tauri CLI）。它同样会触发 `beforeDevCommand`，因此**必须先在
> PATH 里能找到 `npm`**，并已在运行 `tauri-vite.cmd`，否则会因端口冲突或找不到 npm 而失败。
> 若你在「Developer PowerShell for VS」这类环境里 `npm` 可用，`npm run tauri:dev` 也可以。
>
> ⚠️ 三个 `.cmd` 文件**必须保持纯 ASCII**：`cmd.exe` 按控制台 OEM 代码页解析脚本，
> 非 ASCII 字符会破坏行结构（中文注释会导致脚本整体解析失败）。

### 直接用 npm 脚本

```bash
npm install            # 安装前端依赖
npm run typecheck      # vue-tsc 类型检查
npm run dev            # 只跑前端（浏览器里看不到数据，IPC 不可用）
npm run tauri:dev      # 完整桌面应用（需 npm 在 PATH 中）
npm run tauri:build    # 打包安装程序（NSIS）
```

## 5. 后端命令清单（前端可调用的全部 IPC）

| command | 作用 | 是否写操作 |
|---|---|---|
| `supported_managers` | 静态列出支持的管理器与允许的操作 | 否 |
| `detect_managers` | 探测可执行文件/版本/全局目录/缓存目录 | 否（结果缓存 5 分钟） |
| `get_registry` / `get_all_registries` | 读取镜像源配置 | 否 |
| `preview_registry_change` | 预览保存后的配置文件内容 | 否 |
| `set_registry` | 写回镜像源（先备份） | **是** |
| `run_scan` | 完整扫描，返回报告 | 否 |
| `get_cache_stats` | 单个管理器的缓存占用 + 一级子目录分布 | 否 |
| `list_clean_candidates` | 枚举清理候选 | 否 |
| `clean_caches` | 清理；`dryRun` 默认 `true` | **是**（`dryRun=false` 时） |
| `export_report` | 导出 JSON / CSV / Markdown | **是**（写新文件） |
| `get_diagnostics` / `parent_dir` | 环境信息 / 取父目录 | 否 |

## 6. 已支持的包管理器

| id | 语言 | 全局包识别方式 | 缓存目录 |
|---|---|---|---|
| `npm` | Node.js | `npm root -g` + 扫描目录下 package.json | `%LOCALAPPDATA%\npm-cache` |
| `pnpm` | Node.js | `pnpm root -g` | `%LOCALAPPDATA%\pnpm\store` |
| `yarn` | Node.js | `yarn global dir` | `%LOCALAPPDATA%\Yarn\Cache` |
| `bun` | Node.js | 扫描 `~/.bun/install/global/node_modules` | `~/.bun/install/cache` |
| `pip` | Python | `pip list --format=json` + dist-info 定位 | `%LOCALAPPDATA%\pip\Cache` |
| `uv` | Python | `uv pip list --format=json` | `%LOCALAPPDATA%\uv` |
| `cargo` | Rust | `cargo install --list` | `~/.cargo/registry` |
| `go` | Go | 扫描 `GOMODCACHE`（`module@version`） | `~/go/pkg/mod` |
| `gem` | Ruby | `gem list --local` | `~/.gem` |

新增一个管理器只需在 `whitelist.rs` 的 `MANAGERS` 里加一条定义，并在 `packages.rs` /
`manager.rs` 里补该生态的解析分支；前端无需改动（侧边栏与表格按数据驱动渲染）。

## 7. 已知限制

- 项目**局部包**扫描尚未实现：当前 `scope` 只产出 `global`（pip/npm 全局）与 `system`（解释器基础环境）。
  规划中的实现是在用户指定根目录内向上查找 `package-lock.json` / `pyproject.toml` 等标记文件。
- 「最新版本」比对未实现（`latestVersion` 字段恒为 `null`），因为需要联网查询仓库元数据。
- 缓存体积统计对超大目录（如 Go module cache）会触发 `MAX_SCAN_ENTRIES` 截断，此时 `truncated = true`。
- 清理受保护项：pnpm store、uv cache、cargo registry、Go mod cache 的内容寻址/巨大目录默认
  只允许清理其下的缓存子目录；pnpm/cargo/uv/go 的整个存储根不可一键删除（避免破坏项目硬链接）。
- `src-tauri/icons/` 内为脚本生成的占位图标，正式发布前请用 `npx tauri icon <你的 1024px 图>` 替换。

## 8. 测试

```bash
cd src-tauri && cargo test    # 覆盖：输入校验/路径逃逸/白名单/cache 解析/CSV 与 JSON 解析
npm run typecheck             # 前端类型与数据契约校验
```

安全相关的单元测试集中在 `validate.rs`（注入攻击、路径逃逸）、`whitelist.rs`（未知操作被拒）、
`cleaner.rs`（禁止目录名、候选 id 稳定性、dry-run 无副作用）。

### 当前验证状态

| 项目 | 命令 | 结果 |
|---|---|---|
| Rust 编译（含测试目标） | `cargo check --all-targets` | ✅ 通过，0 告警 |
| Rust 单元测试 | `cargo test --lib` | ✅ 26 passed / 0 failed（含对已安装管理器的端到端命令执行） |
| 前端类型检查 | `npx vue-tsc --noEmit` | ✅ 通过 |
| 前端生产构建 | `npx vite build` | ✅ 通过（JS 113 KB / gzip 42 KB） |

尚未验证：`npm run tauri:dev` 的完整窗口启动与真实 GUI 交互、`npm run tauri:build` 打包安装程序
（需要 WebView2 运行时与更长时间的首轮 release 编译）。
