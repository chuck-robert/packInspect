<#
.SYNOPSIS
    PackInspect 构建 / 测试 / 启动辅助脚本。

.DESCRIPTION
    自行完成环境预检与工具链注入（见 env-preflight.ps1），因此不依赖当前 shell 的 PATH。
    所有 cargo 输出都会原样透传到控制台并同时写入日志，方便看出「还在编译」而不是卡死。

.EXAMPLE
    ./scripts/build.ps1 check          # cargo check --all-targets（最快，不链接）
    ./scripts/build.ps1 test           # cargo test --lib
    ./scripts/build.ps1 clippy         # cargo clippy --all-targets
    ./scripts/build.ps1 build          # 只编译可执行文件（不启动）
    ./scripts/build.ps1 run            # 编译并启动桌面应用
    ./scripts/build.ps1 test -- plugins --nocapture   # `--` 之后的参数原样透传给 cargo
#>
[CmdletBinding()]
param(
    [ValidateSet('check', 'test', 'clippy', 'run', 'build', 'package', 'doctor', 'verify')]
    [string]$Task = 'check',

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs = @()
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'env-preflight.ps1')

Write-Host ''
Write-Host '  PackInspect' -ForegroundColor Cyan -NoNewline
Write-Host ' 本地包环境扫描' -ForegroundColor DarkGray
# 必须整体加括号：`Write-Host a + b` 会被解析成「两个位置参数组成的数组」，
# 输出时 PowerShell 用 '+' 连接，结果就多出一个加号。
Write-Host ('  ' + ('-' * 64)) -ForegroundColor DarkGray

# ---------------------------------------------------------------------------
# 环境预检
# ---------------------------------------------------------------------------
$toolchain = Get-PackInspectToolchain

if ($Task -eq 'doctor') {
    Write-Host ''
    Write-Host '  环境检查结果' -ForegroundColor Cyan
    Write-Host "    Node.js     : $(if (Get-Command node -EA SilentlyContinue) { (node --version) } else { '缺失' })"
    Write-Host "    cargo       : $(if ($toolchain.Cargo) { $toolchain.Cargo } else { '缺失' })"
    Write-Host "    Visual Studio: $(if ($toolchain.VsPath) { $toolchain.VsPath } else { '缺失（无 C++ 工具链）' })"
    if ($toolchain.Problems.Count) {
        Write-Host ''
        Write-Host '  阻塞项：' -ForegroundColor Red
        $toolchain.Problems | ForEach-Object { Write-Host "    - $_" -ForegroundColor Red }
    }
    if ($toolchain.Hints.Count) {
        Write-Host ''
        Write-Host '  建议：' -ForegroundColor Yellow
        $toolchain.Hints | ForEach-Object { Write-Host "    - $_" -ForegroundColor Yellow }
    }
    Write-Host ''
    exit $(if ($toolchain.Ok) { 0 } else { 1 })
}

if (-not $toolchain.Ok) {
    Write-Host ''
    Write-Host '  ✗ 环境不满足，无法继续' -ForegroundColor Red
    $toolchain.Problems | ForEach-Object { Write-Host "    - $_" -ForegroundColor Red }
    if ($toolchain.Hints.Count) {
        Write-Host ''
        Write-Host '  请先完成：' -ForegroundColor Yellow
        $toolchain.Hints | ForEach-Object { Write-Host "    - $_" -ForegroundColor Yellow }
    }
    Write-Host ''
    Write-Host '  也可以随时单独体检：./scripts/build.ps1 doctor' -ForegroundColor DarkGray
    Write-Host ''
    exit 1
}

# ---------------------------------------------------------------------------
# package：产出安装程序（NSIS 向导式）
# ---------------------------------------------------------------------------
# 打包前先跑一次 verify：配置类问题（BOM、缺 resources、v1 字段名）在这里
# 报错比在 tauri build 里报错清楚得多，而且打包要几分钟，早失败更省时间。
if ($Task -eq 'package') {
    Write-Host ''
    Write-Host '  ▶ 打包前先做配置校验' -ForegroundColor Cyan
    & $PSCommandPath verify
    if ($LASTEXITCODE -ne 0) {
        Write-Host ''
        Write-Host '  ✗ 配置校验未通过，已中止打包' -ForegroundColor Red
        exit 1
    }

    Write-Host ''
    Write-Host '  ▶ npx tauri build' -ForegroundColor Cyan
    Write-Host '    首次打包需要编译 release 版（3~8 分钟），请勿关闭窗口。' -ForegroundColor DarkGray
    Write-Host ''
    Push-Location $repoRoot
    try {
        & cmd.exe /c "npx tauri build 2>&1"
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
    }

    if ($code -ne 0) {
        Write-Host ''
        Write-Host "  ✗ 打包失败（退出码 $code）" -ForegroundColor Red
        exit $code
    }

    # 把产物路径明确打出来 —— tauri 的输出夹在一堆编译日志里，不好找
    $bundleDir = Join-Path $repoRoot 'src-tauri\target\release\bundle'
    $setup = Get-ChildItem $bundleDir -Recurse -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Write-Host ''
    if ($setup) {
        Write-Host "  ✓ 安装程序：$($setup.FullName)" -ForegroundColor Green
        Write-Host "    大小：$([math]::Round($setup.Length / 1MB, 2)) MB" -ForegroundColor DarkGray
    } else {
        Write-Host '  ✓ 打包完成，但没找到 *-setup.exe，请检查 bundle 目录' -ForegroundColor Yellow
    }
    Write-Host ''
    exit 0
}

# ---------------------------------------------------------------------------
# verify：前端纯逻辑的回归校验
# ---------------------------------------------------------------------------
# 这几个检查覆盖的是「只有跑真实数据才会暴露」的前端逻辑问题，而它们又不需要
# 浏览器或 Tauri 运行时（逻辑是纯函数），因此用 Node 直接跑最省事：
#   verify-scan-merge  单管理器扫描不得清空其它管理器的数据
#   verify-search-parity 包列表分页与顶部搜索必须用同一套匹配规则
# 不放进 cargo test 是因为它们是 TypeScript 侧的逻辑；不放进 vitest 是因为
# 为了两个断言引入整套测试框架不划算（项目目前无前端测试依赖）。
if ($Task -eq 'verify') {
    $scripts = @('verify-scan-merge.mjs', 'verify-search-parity.mjs', 'verify-config.mjs')
    $node = (Get-Command node -ErrorAction SilentlyContinue).Source
    if (-not $node) {
        Write-Host '  找不到 node，无法执行前端逻辑校验。' -ForegroundColor Red
        exit 1
    }
    $failed = 0
    foreach ($s in $scripts) {
        $path = Join-Path $repoRoot "scripts\$s"
        Write-Host ''
        Write-Host "  ▶ node scripts\$s" -ForegroundColor Cyan
        # 用 cmd 调用并把输出直接透传，保留脚本自身的退出码
        & cmd.exe /c "`"$node`" `"$path`""
        if ($LASTEXITCODE -ne 0) { $failed++ }
    }
    Write-Host ''
    if ($failed -gt 0) {
        Write-Host "  ✗ 失败（$failed 个校验未通过）" -ForegroundColor Red
        exit 1
    }
    Write-Host '  ✓ 完成' -ForegroundColor Green
    exit 0
}

# ---------------------------------------------------------------------------
# 组装 cargo 命令
# ---------------------------------------------------------------------------
$cargoCmd = switch ($Task) {
    'test' { @('test', '--lib') }
    'clippy' { @('clippy', '--all-targets') }
    'run' { @('run', '--no-default-features', '--color', 'always') }
    'build' { @('build', '--no-default-features', '--color', 'always') }
    default { @('check', '--all-targets', '--color', 'always') }
}
$full = $cargoCmd + $CargoArgs

$logDir = Join-Path $repoRoot '.logs'
if (-not (Test-Path $logDir)) { [void](New-Item -ItemType Directory -Path $logDir) }
$logPath = Join-Path $logDir 'build.log'
$exePath = Join-Path $repoRoot 'src-tauri\target\debug\packinspect.exe'

# 首次编译 400+ 个 crate 需要数分钟；明确告知，避免被误认为卡死
if ($Task -in @('run', 'build') -and -not (Test-Path $exePath)) {
    Write-Host ''
    Write-Host '  首次编译需要下载并编译 400 多个依赖 crate，通常 3~8 分钟。' -ForegroundColor Yellow
    Write-Host '  期间会持续输出 Compiling / Building 进度，请不要关闭窗口。' -ForegroundColor Yellow
}

Write-Host ''
Write-Host "  ▶ cargo $($full -join ' ')" -ForegroundColor Cyan
Write-Host "    完整日志：$logPath" -ForegroundColor DarkGray
Write-Host ''

# 为什么不用 `& cargo ... 2>&1 | Tee-Object`：
# cargo 把进度（Compiling / Building / Finished）写到 **stderr**，而 PowerShell 会把
# 原生命令的 stderr 包装成 ErrorRecord 显示成红色 `NativeCommandError` —— 看起来像失败，
# 实际退出码是 0。这里改用 ProcessStartInfo 把 stdout/stderr 原样重定向到日志文件，
# 控制台只输出干净的摘要，从根上避免这种误报。
$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $toolchain.Cargo
# 注意：不能用 ArgumentList —— 它在 .NET Framework（Windows PowerShell 5.1）上不存在，
# 而启动器正是用 powershell.exe 调起本脚本的。这里手工拼 Parameters，
# 并对含空格的参数补引号（cargo 参数通常不含空格，但路径可能含）。
$quoted = $full | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }
$psi.Arguments = ($quoted -join ' ')
$psi.WorkingDirectory = Join-Path $repoRoot 'src-tauri'
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.CreateNoWindow = $true
# cargo 会用 ANSI 颜色；日志文件里不要残留转义序列
$psi.EnvironmentVariables['CARGO_TERM_COLOR'] = 'never'

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $psi

# 日志文件可能被上一次仍在运行的构建占用，此时退回到带时间戳的名字
$stream = $null
try {
    $stream = [System.IO.File]::Create($logPath)
} catch {
    $logPath = Join-Path $logDir ("build-{0}.log" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
    Write-Host "  ! 默认日志被占用，改用 $logPath" -ForegroundColor Yellow
    $stream = [System.IO.File]::Create($logPath)
}
$writer = [System.IO.StreamWriter]::new($stream)
$writer.AutoFlush = $true

try {
    [void]$process.Start()
    # 两个流同时读，避免任一管道写满导致 cargo 阻塞
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $text = $stdout.Result + $stderr.Result
    $writer.Write($text)
    $code = $process.ExitCode
} finally {
    $writer.Dispose()
    $stream.Dispose()
    $process.Dispose()
}

# 控制台只回显有信息量的行，避免刷屏（完整内容始终在日志文件里）
$lines = $text -split "`r?`n"
$errors = $lines | Where-Object { $_ -match '^\s*error(\[|:)' }
$compileCount = ($lines | Where-Object { $_ -match '^\s*(Compiling|Checking|Building)' }).Count
$finished = $lines | Where-Object { $_ -match '^\s*Finished' } | Select-Object -Last 1

if ($compileCount -gt 0) {
    Write-Host "  -- 编译了 $compileCount 步 --" -ForegroundColor DarkGray
}
if ($finished) { Write-Host "    $($finished.Trim())" -ForegroundColor DarkGray }
if ($errors.Count -gt 0) {
    Write-Host '  -- 错误 --' -ForegroundColor Red
    $errors | Select-Object -First 15 | ForEach-Object { Write-Host "    $($_.Trim())" -ForegroundColor Red }
}

Write-Host ''
if ($code -eq 0) {
    Write-Host '  ✓ 完成' -ForegroundColor Green
} else {
    Write-Host "  ✗ 失败（退出码 $code）" -ForegroundColor Red
    Write-Host "    排障建议：先看 $logPath 里的 error 行；环境问题可用 ./scripts/build.ps1 doctor 体检" -ForegroundColor DarkGray
}
Write-Host ''
exit $code
