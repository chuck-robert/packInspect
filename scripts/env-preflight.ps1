<#
.SYNOPSIS
    PackInspect 启动前的环境预检与工具链定位。

.DESCRIPTION
    被 启动 PackInspect.cmd 与 build.ps1 共用。做两件事：
      1. 检查 node / cargo / MSVC 链接器是否具备，缺失时给出**可执行**的修复建议
      2. 注入 cargo 路径 + MSVC 环境变量（vcvars64.bat 的 PATH 采用合并而非替换）

    dot-source 使用：
        . "$PSScriptRoot\env-preflight.ps1"
        $env = Get-PackInspectToolchain
#>

function Get-VisualStudioPath {
    <# 用 vswhere 定位带 C++ 工具链的 VS 安装，失败再退回常见路径 #>
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $found = & $vswhere -latest -products * `
            -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
            -property installationPath 2>$null
        if ($found) { return $found.Trim() }
    }
    $candidates = @(
        'D:\Microsoft Visual Studio\18\Community',
        'C:\Program Files\Microsoft Visual Studio\2022\Community',
        'C:\Program Files\Microsoft Visual Studio\2022\Professional',
        'C:\Program Files\Microsoft Visual Studio\2022\BuildTools',
        'C:\Program Files (x86)\Microsoft Visual Studio\2019\BuildTools'
    )
    foreach ($candidate in $candidates) {
        if (Test-Path (Join-Path $candidate 'VC\Auxiliary\Build\vcvars64.bat')) { return $candidate }
    }
    return $null
}

function Import-VcVarsEnvironment {
    <#
        执行 vcvars64.bat 并把结果导入当前会话。
        关键：PATH 采用「原始用户 PATH + vcvars 追加目录」的**合并**方式。
        vcvars 会把 PATH 重置成它自己的一套，直接采用会丢掉 nodejs / python 等目录，
        导致应用运行时探测不到 npm、pip（本项目实际踩过这个坑）。
    #>
    param([string]$VsPath)

    $vcvars = Join-Path $VsPath 'VC\Auxiliary\Build\vcvars64.bat'
    if (-not (Test-Path $vcvars)) { return $false }

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
    $vcvarsPathLine = $output | Where-Object { $_ -match '^PATH=' } | Select-Object -First 1
    if ($vcvarsPathLine) {
        $merged = $userPath
        foreach ($dir in ($vcvarsPathLine -replace '^PATH=', '').Split(';')) {
            if ($dir -and $merged -notlike "*$dir*") { $merged = "$merged;$dir" }
        }
        $env:PATH = $merged
    }
    return $true
}

function Get-PackInspectToolchain {
    <#
        返回 @{ Ok; Cargo; Problems; Hints }
        Ok = $false 时 Problems 是阻塞项，Hints 是给用户的修复动作。
    #>
    $problems = [System.Collections.Generic.List[string]]::new()
    $hints = [System.Collections.Generic.List[string]]::new()

    # ---- Node（前端 devServer 必需）----
    $node = Get-Command node -ErrorAction SilentlyContinue
    if (-not $node) {
        $problems.Add('未找到 Node.js（node 命令不可用）')
        $hints.Add('安装 Node.js LTS：https://nodejs.org/en/download  装完重开一个终端')
    }

    # ---- Rust ----
    $cargoHome = Join-Path $env:USERPROFILE '.cargo'
    $cargoExe = Join-Path $cargoHome 'bin\cargo.exe'
    if (-not (Test-Path $cargoExe)) {
        $onPath = Get-Command cargo -ErrorAction SilentlyContinue
        $cargoExe = if ($onPath) { $onPath.Source } else { $null }
    }
    if (-not $cargoExe) {
        $problems.Add('未找到 Rust 工具链（cargo 命令不可用）')
        $hints.Add('安装 Rust：https://rustup.rs/  或下载 rustup-init.exe 运行后重开终端')
        $hints.Add('网络受限时先设代理：$env:HTTPS_PROXY="http://127.0.0.1:7890"')
    }

    # ---- MSVC 链接器（cargo 链接必需）----
    $vsPath = Get-VisualStudioPath
    if (-not $vsPath) {
        $problems.Add('未找到 Visual Studio C++ 生成工具（缺 link.exe，cargo 无法完成链接）')
        $hints.Add('安装「使用 C++ 的桌面开发」工作负载：https://visualstudio.microsoft.com/visual-cpp-build-tools/')
    } else {
        [void](Import-VcVarsEnvironment -VsPath $vsPath)
    }

    # ---- WebView2（运行时必需）----
    $webview = Get-ItemProperty `
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' `
        -ErrorAction SilentlyContinue
    if (-not $webview) {
        $webview = Get-ItemProperty `
            'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' `
            -ErrorAction SilentlyContinue
    }
    if (-not $webview) {
        # 注册表没查到不算阻塞：Win10/11 通常已内置，只在失败时才有意义
        $hints.Add('未在注册表查到 WebView2 运行时；若应用窗口空白，请安装 https://developer.microsoft.com/microsoft-edge/webview2/')
    }

    # 保证 cargo 在 PATH 中（vcvars 处理完后仍然可用）
    if ($cargoExe) { $env:PATH = "$(Split-Path $cargoExe);$env:PATH" }

    [pscustomobject]@{
        Ok       = ($problems.Count -eq 0)
        Cargo    = $cargoExe
        VsPath   = $vsPath
        Problems = $problems
        Hints    = $hints
    }
}
