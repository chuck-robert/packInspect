<#
.SYNOPSIS
    PackInspect 一键启动：确保依赖与 devServer 就绪，然后编译并启动桌面应用。

.DESCRIPTION
    被根目录的「启动 PackInspect.cmd」调用。放在 PowerShell 里而不是批处理里，
    是因为 cmd.exe 按字节解析 .cmd（必须 CRLF + 纯 ASCII），稍有不慎就会
    把脚本拆成 "'M' is not recognized" 之类的碎片，难以排查。

.NOTES
    退出码：0 应用正常退出；非 0 表示启动失败（详情见 .logs\build.log）
#>
[CmdletBinding()]
param(
    # 跳过 devServer 检查（仅在已有服务在跑时用）
    [switch]$SkipDevServer
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot

function Write-Step {
    param([string]$Text, [string]$Color = 'Gray')
    Write-Host "  $Text" -ForegroundColor $Color
}

Write-Host ''
Write-Host '  PackInspect' -ForegroundColor Cyan -NoNewline
Write-Host ' 本地包环境扫描' -ForegroundColor DarkGray
Write-Host ('  ' + ('=' * 62)) -ForegroundColor DarkGray
Write-Host ''

Set-Location $repoRoot

# ---------------------------------------------------------------- 1. 依赖
$viteBin = Join-Path $repoRoot 'node_modules\vite\bin\vite.js'
if (-not (Test-Path $viteBin)) {
    Write-Step '[1/3] 缺少依赖，正在执行 npm install（首次可能需要几分钟）' 'Yellow'
    & npm install
    if ($LASTEXITCODE -ne 0) {
        Write-Host ''
        Write-Step '[X] npm install 失败。请检查网络或代理，也可尝试：' 'Red'
        Write-Step '    npm install --registry=https://registry.npmmirror.com' 'Red'
        Write-Host ''
        Read-Host '按回车键退出'
        exit 1
    }
}
Write-Step '[1/3] 依赖就绪' 'DarkGray'

# ------------------------------------------------- 2. devServer（端口 1420）
$logDir = Join-Path $repoRoot '.logs'
if (-not (Test-Path $logDir)) { [void](New-Item -ItemType Directory -Path $logDir) }
$viteLog = Join-Path $logDir 'vite-dev.log'

function Test-DevServer {
    <#
        探活 devServer。
        依次尝试 IPv6 ::1、IPv4 127.0.0.1、以及一次真实 HTTP 请求 ——
        Vite 默认监听 localhost，在部分 Windows 上只解析到 ::1，
        只探 127.0.0.1 会一直失败，表现为「启动器卡死」。
    #>
    foreach ($host_ in @('::1', '127.0.0.1')) {
        try {
            $client = [System.Net.Sockets.TcpClient]::new()
            $task = $client.ConnectAsync($host_, 1420)
            if ($task.Wait(600) -and $client.Connected) { $client.Dispose(); return $true }
            $client.Dispose()
        } catch { }
    }
    try {
        $response = Invoke-WebRequest -Uri 'http://localhost:1420/' -TimeoutSec 2 -UseBasicParsing
        return ($response.StatusCode -eq 200)
    } catch {
        return $false
    }
}

if ($SkipDevServer) {
    Write-Step '[2/3] 已跳过 devServer 检查' 'DarkGray'
} elseif (Test-DevServer) {
    Write-Step '[2/3] devServer 已在运行' 'DarkGray'
} else {
    Write-Step '[2/3] 启动 devServer …' 'DarkGray'
    Start-Process -FilePath 'cmd.exe' `
        -ArgumentList '/c', "node `"$viteBin`" > `"$viteLog`" 2>&1" `
        -WorkingDirectory $repoRoot -WindowStyle Minimized | Out-Null

    $ready = $false
    for ($i = 0; $i -lt 60; $i++) {
        Start-Sleep -Milliseconds 500
        if (Test-DevServer) { $ready = $true; break }
        if ($i % 10 -eq 9) { Write-Host '.' -NoNewline -ForegroundColor DarkGray }
    }
    Write-Host ''
    if (-not $ready) {
        Write-Step '[X] devServer 60 秒内未就绪' 'Red'
        Write-Step "    详情见 $viteLog" 'Red'
        Write-Step '    常见原因：端口 1420 被其他程序占用' 'Red'
        Write-Host ''
        Read-Host '按回车键退出'
        exit 1
    }
    Write-Step '[2/3] devServer 就绪' 'DarkGray'
}

# ---------------------------------------------------- 3. 编译并启动应用
Write-Step '[3/3] 编译并启动桌面应用 …' 'DarkGray'
Write-Step '      首次需编译 400+ 个依赖 crate，通常 3~8 分钟，请勿关闭窗口' 'Yellow'
Write-Host ''

& (Join-Path $PSScriptRoot 'build.ps1') run
$code = $LASTEXITCODE

Write-Host ''
if ($code -eq 0) {
    Write-Step '应用已退出。' 'Green'
} else {
    Write-Step "[X] 启动失败（退出码 $code）" 'Red'
    Write-Step "    - 环境体检：powershell -File scripts\build.ps1 doctor" 'Red'
    Write-Step "    - 完整日志：$logDir\build.log" 'Red'
}
Write-Host ''
Write-Step 'devServer 仍在最小化窗口中运行；要停止它请关闭该窗口。' 'DarkGray'
Write-Host ''
Read-Host '按回车键退出'
exit $code
