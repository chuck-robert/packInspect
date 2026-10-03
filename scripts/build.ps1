<#
.SYNOPSIS
    PackInspect 构建/测试辅助脚本（替代 .cmd 版本，避免 cmd.exe 的引号解析问题）。

.EXAMPLE
    ./scripts/build.ps1 check          # cargo check --all-targets（最快，不链接）
    ./scripts/build.ps1 test           # cargo test --lib
    ./scripts/build.ps1 clippy         # cargo clippy --all-targets
    ./scripts/build.ps1 run            # 编译并启动桌面应用
    ./scripts/build.ps1 test plugins   # 透传过滤参数给 cargo
#>
[CmdletBinding()]
param(
    [ValidateSet('check', 'test', 'clippy', 'run', 'build')]
    [string]$Task = 'check',

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$CargoArgs = @()
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

# ---- 工具链环境 ------------------------------------------------------------
$env:CARGO_HOME = Join-Path $env:USERPROFILE '.cargo'
$env:RUSTUP_HOME = Join-Path $env:USERPROFILE '.rustup'
$env:PATH = "$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"

# 网络受限时启用本地代理（Clash 默认 7890）
$proxy = 'http://127.0.0.1:7890'
if (-not $env:HTTP_PROXY) { $env:HTTP_PROXY = $proxy }
if (-not $env:HTTPS_PROXY) { $env:HTTPS_PROXY = $proxy }

# ---- MSVC 链接器环境（cargo 需要 link.exe）---------------------------------
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vsPath = $null
if (Test-Path $vswhere) {
    $vsPath = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $vsPath) {
    # vswhere 不可用时的常见路径兜底
    $candidates = @(
        'D:\Microsoft Visual Studio\18\Community',
        'C:\Program Files\Microsoft Visual Studio\2022\Community',
        'C:\Program Files\Microsoft Visual Studio\2022\BuildTools'
    )
    $vsPath = $candidates | Where-Object { Test-Path (Join-Path $_ 'VC\Auxiliary\Build\vcvars64.bat') } | Select-Object -First 1
}

if ($vsPath) {
    $vcvars = Join-Path $vsPath 'VC\Auxiliary\Build\vcvars64.bat'
    if (Test-Path $vcvars) {
        Write-Host "[build] 加载 MSVC 环境: $vsPath" -ForegroundColor DarkGray
        # 先备份用户 PATH：vcvars64.bat 会把 PATH 重置为它自己的一套，
        # 那会丢掉 nodejs / python 等目录，导致应用运行时探测不到 npm、pip。
        $userPath = $env:PATH
        $output = & cmd.exe /c "`"$vcvars`" >nul 2>&1 && set"
        foreach ($line in $output) {
            if ($line -match '^([^=]+)=(.*)$') {
                $name = $Matches[1]
                if ($name -match '^(INCLUDE|LIB|LIBPATH|VCINSTALLDIR|VCToolsInstallDir|WindowsSdkDir|WindowsSDKVersion|UCRTVersion|VSINSTALLDIR|VSCMD_.*)$') {
                    Set-Item -Path "env:$name" -Value $Matches[2] -ErrorAction SilentlyContinue
                }
            }
        }
        # PATH 采用「原始用户 PATH + vcvars 追加的目录」，避免丢失开发工具
        $vcvarsPath = ($output | Where-Object { $_ -match '^PATH=' } | Select-Object -First 1)
        if ($vcvarsPath) {
            $merged = $userPath
            foreach ($dir in ($vcvarsPath -replace '^PATH=', '').Split(';')) {
                if ($dir -and $merged -notlike "*$dir*") { $merged = "$merged;$dir" }
            }
            $env:PATH = $merged
        }
    }
} else {
    Write-Warning "[build] 未找到 Visual Studio C++ 工具链；若编译报 'link.exe not found'，请安装 MSVC Build Tools。"
}

$cargoExe = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
if (-not (Test-Path $cargoExe)) {
    # 退回到 PATH 查找
    $found = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $found) {
        throw "找不到 cargo。请先安装 Rust 工具链：https://rustup.rs/"
    }
    $cargoExe = $found.Source
}
# vcvars64.bat 会重置 PATH，因此用绝对路径调用 cargo，保证顺序无关
$env:PATH = "$(Join-Path $env:CARGO_HOME 'bin');$env:PATH"

Push-Location (Join-Path $repoRoot 'src-tauri')
try {
    $cargoCmd = switch ($Task) {
        'test' { @('test', '--lib') }
        'clippy' { @('clippy', '--all-targets') }
        'run' { @('run', '--no-default-features', '--color', 'never') }
        'build' { @('build', '--no-default-features') }
        default { @('check', '--all-targets') }
    }
    $full = $cargoCmd + $CargoArgs
    Write-Host "[build] cargo $($full -join ' ')" -ForegroundColor Cyan
    # 关键：cargo 会把进度写到 stderr，而 PowerShell 在 $ErrorActionPreference='Stop'
    # 下会把原生命令的 stderr 当成终止错误。这里临时放宽，只认退出码。
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $cargoExe @full
        $code = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $prevEap
    }
    Write-Host "[build] exit code: $code" -ForegroundColor $(if ($code -eq 0) { 'Green' } else { 'Red' })
    exit $code
} finally {
    Pop-Location
}
