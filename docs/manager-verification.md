# 新增包管理器的能力核实记录

> 这份文件记录**每个管理器的真实能力边界**，以及哪些命令是实测的、哪些只能靠文档。
> 目的：录入白名单时不会因为"想让界面好看"而编造不存在的命令。
>
> 核实方式：4 个子代理查官方文档 + 上游源码。**注意：除 npm 外，本机未安装这些管理器，
> 因此绝大多数命令没有实际执行过**（见每条的「证据强度」）。

## 证据强度分级

| 级别 | 含义 |
|---|---|
| **实测** | 在本机真实执行过，看到输出 |
| **文档** | 官方文档明确写出，可信度高 |
| **源码** | 官方文档没写，从上游源码读出，需运行时探测确认 |
| **存疑** | 文档与源码不一致，或只在部分版本存在 |

---

## 语言 / 运行时生态

### bun
| 项 | 内容 | 证据 |
|---|---|---|
| 平台 | all | 文档 |
| 可执行 | `bun.exe`, `bun.cmd`, `bun` | 文档 |
| version | `--version` | 文档 |
| 列出已安装 | `pm ls -g` | **源码**（官方文档未写，需探测） |
| 安装 | `add -g {}` | 文档 |
| 卸载 | `remove -g {}` | **存疑**（`--global` 只在 usage 块出现，正文无示例） |
| 更新 | `update -g {}` | 文档 |
| 缓存 | `~/.bun/install/cache`；全局目录 `~/.bun/install/global`；bin `~/.bun/bin` | 文档 |

**风险**：`bun pm cache rm` 会清掉**所有项目**的缓存；`bun pm trust --all` 会执行所有被阻止的生命周期脚本（任意代码）。

**处理**：`pm ls -g` 若探测失败，退路是解析 `~/.bun/install/global/package.json`。
`remove -g` 标为「可用但未充分验证」，界面上照常给出命令，但确认框里注明。

### deno
| 项 | 内容 | 证据 |
|---|---|---|
| 平台 | all | 文档 |
| 可执行 | `deno.exe`, `deno.cmd`, `deno` | 文档 |
| version | `--version` | 文档 |
| 列出已安装 | **不存在全局列表命令** | 文档 + 源码 |
| 列出项目依赖 | `list` | **存疑**（`deno list` 2026-06 才加，旧版没有） |
| 安装（全局） | `install -g -A {}` | 文档 |
| 卸载（全局） | `uninstall -g {}` | 文档 |
| 更新 | **无全局更新命令**，只能重装 | 文档 |
| 缓存 | `DENO_DIR`：Win `%LOCALAPPDATA%\deno`；bin `~/.deno/bin` | 文档 |

**重要**：deno 1.x 与 2.x 语义相反。1.x 里 `deno install` **默认就是全局**脚本安装；
2.x 才改成「默认装项目依赖、全局要 `-g`」。因此 `install`/`uninstall` 的参数在不同大版本上含义不同，
不能假设行为一致。

**处理**：
- 「列出已安装」= 枚举全局 bin 目录（`$DENO_INSTALL_ROOT` 或 `~/.deno/bin`），
  跳过 dotfile 与 `.exe` 同伴文件，对 `<name>` / `<name>.cmd` 去重。
  这是唯一可靠途径，且必须在界面上说明「deno 无官方列表命令，此处按已安装的可执行文件统计」。
- 「更新」标为**不可用**，原因写明。
- `deno list` 需要运行时探测（`deno list --help` 退出码）后再决定是否启用。

### Julia Pkg
| 项 | 内容 | 证据 |
|---|---|---|
| 平台 | all | 文档 |
| 可执行 | `julia.exe`, `julia` | 文档 |
| version | `--version` | 文档 |
| 列出已安装 | `-e "using Pkg; Pkg.status()"` | 文档 |
| 安装 | `-e "using Pkg; Pkg.add(\"{}\")"` | 文档 |
| 卸载 | `-e "using Pkg; Pkg.rm(\"{}\")"` | 文档 |
| 更新 | `-e "using Pkg; Pkg.update()"` | 文档 |
| depot | `~/.julia`（`JULIA_DEPOT_PATH`） | 文档 |

**重大风险（必须处理）**：`Pkg.rm` **不删除文件**，只改 Project.toml；真正的删除是仓库级的
`Pkg.gc()`（清理整个 depot 中 7 天未用的版本与 artifact，Julia 1.12 起还会自动 GC）。
**绝不能把 `Pkg.gc()` 接到「卸载这个包」上** —— 那会清掉用户所有项目共享的内容。

**解析**：`Pkg.status()` 输出形如 `Status \`~/.julia/environments/v1.10/Project.toml\`` 后跟
`  [7876af07] Example v0.5.3`。按 `[uuid] Name vX.Y.Z` 的 token 模式解析，**不要按列位置**。
标记 `⌃`（可升级）/ `⌅`（被 compat 限制）/ `[yanked]` 需容错。空环境输出 `(empty environment)`。
必须设 `NO_COLOR=1` 保证输出稳定。`-e` 与脚本串必须作为**两个独立 argv**（cmd.exe 不认单引号）。

**注意**：默认列的是**活动环境**（`PKGMODE_PROJECT`），不是全局。要说明这是"当前 Julia 环境"。

### Hex / mix
| 项 | 内容 | 证据 |
|---|---|---|
| 平台 | all | 文档 |
| 可执行 | `mix.bat`, `mix`, `mix.ps1` | 文档 |
| version | `hex.info`（无参数，输出系统信息） | 文档 |
| 列出已安装 | **无全局列表**；`mix deps` 是项目级 | 文档 |
| 安装 | **无 `mix hex install`**；需改 mix.exs + `mix deps.get` | 文档 |
| 卸载 | 项目级：`deps.clean {} --unlock`；全局工具：`escript.uninstall {}` | 文档 |
| 更新 | `deps.update {}`（项目级） | 文档 |
| 缓存 | Hex home `~/.hex`（**不是** `~/.mix`）；archives `~/.mix/archives`；escripts `~/.mix/escripts` | 文档 |

**重要**：Hex 本身**不是可执行文件**，它是 `mix local.hex` 装进 `~/.mix/archives` 的 archive，
只能以 `mix hex.*` 形式调用。因此探测目标应是 `mix` 而非 `hex`。

**处理**：
- 「列出已安装」标为**不可用**并说明：Elixir 的项目依赖是项目级的，没有全局已安装列表。
- 「安装」标为**不可用**并说明：需要编辑 mix.exs 后 `mix deps.get`，不是一条命令能完成的。
  （全局 escript 安装是另一回事，且会执行代码 —— 不适合替用户做。）
- 「卸载」仅对全局 escript 有意义，标注清楚作用域。

---

## 待补：其余 3 个代理的报告

- 系统 / 构建工具：Gradle、Homebrew、vcpkg、Conan、SPM、CocoaPods
- Linux 发行版：apt、pacman、dnf、yum、flatpak、snap、pipx
- 语言工具链：opam、dub、nimble、cabal、stack
