# 在可见的命令行窗口里执行包管理操作，同时把输出写入日志文件。
#
# 为什么需要这个包装脚本：
#   用户希望在安装时能看到实时进度（下载到哪一步、报了什么错），因此进程必须
#   跑在一个**可见的控制台**里。但可见控制台里子进程的输出直接给了用户，
#   父进程拿不到 —— 于是这里把输出写一份到日志文件，
#   PackInspect 读这个文件就能把结果回显到界面。
#
# 为什么是「先捕获、再落盘 + 打印」而不是 Tee-Object 边跑边写：
#   踩过的坑：PowerShell 5.1 的 Tee-Object 会缓冲输出，直到 PowerShell 进程本身
#   退出才把内容刷到文件 —— 而 PowerShell 退出时 PackInspect 已经读完日志了，
#   结果日志总是空的（命令行窗口里却明明有输出）。
#   改成「等命令跑完，把输出收进变量，再写日志并打印到控制台」之后，
#   写状态的时刻一定晚于写日志的时刻，父进程读到的就是完整内容。
#   代价是输出不再逐行实时滚动，而是命令结束时一次性显示 —— 对装机场景可以接受。
#
# 为什么用 JSON 传参数：
#   把参数拼成「逗号分隔的单个字符串」传给脚本是行不通的 —— 逗号数组字面量只在
#   PowerShell 脚本内部有效，命令行上那串东西会被当成**一个字符串**，
#   于是 npm 收到 "install,-g,is-number" 并报 Unknown command。
#   JSON 没有歧义，也不依赖"参数恰好干净"这个前提。
#
# 兼容性：目标是最低 Windows PowerShell 5.1（系统自带）。不要使用只在 PowerShell 7
#   上存在的参数 —— 例如 `Tee-Object -Encoding`。
#
# 用法（由 package_ops.rs 调用，不面向用户直接使用）：
#   powershell -NoProfile -ExecutionPolicy Bypass -File run-install.ps1 `
#       -FilePath C:\path\to\npm.cmd -ArgumentsJson '["install","-g","is-number"]' `
#       -LogPath C:\...\op.log

[CmdletBinding()]
param(
    # 要执行的可执行文件绝对路径
    [Parameter(Mandatory = $true)][string]$FilePath,

    # 参数数组的 JSON 表示，例如 ["install","-g","vue"]
    [Parameter()][string]$ArgumentsJson = '[]',

    # 输出日志路径（界面会读它来回显结果）
    [Parameter(Mandatory = $true)][string]$LogPath,

    # 完成后是否等待按键（默认不等待，见下方说明）
    [Parameter()][switch]$Pause
)

$ErrorActionPreference = 'Continue'

# 退出码单独落一个纯 ASCII 文件，避免与日志混编
$StatusPath = "$LogPath.status"
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Write-Status {
    param([int]$Code)
    try {
        [System.IO.File]::WriteAllText($StatusPath, "$Code", (New-Object System.Text.ASCIIEncoding))
    } catch { }
}

# 先把日志写成空文件：父进程读到"存在但为空"也能与"根本没有日志"区分开
try {
    if (Test-Path $StatusPath) { Remove-Item $StatusPath -Force -ErrorAction SilentlyContinue }
    $logDir = Split-Path -Parent $LogPath
    if ($logDir -and -not (Test-Path $logDir)) {
        New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    }
    [System.IO.File]::WriteAllText($LogPath, '', $utf8NoBom)
} catch { }

function Save-Log {
    param([string]$Text)
    try {
        [System.IO.File]::WriteAllText($LogPath, $Text, $utf8NoBom)
    } catch { }
}

# 先解析参数：失败也要留下状态，否则父进程无法判断发生了什么
$parsed = @()
try {
    if ($ArgumentsJson -and $ArgumentsJson.Trim() -ne '' -and $ArgumentsJson.Trim() -ne '[]') {
        $parsed = @(ConvertFrom-Json -InputObject $ArgumentsJson)
    }
} catch {
    $msg = "PackInspect: 参数解析失败 - $($_.Exception.Message)"
    Write-Host "  $msg" -ForegroundColor Red
    Save-Log $msg
    Write-Status 2
    if ($Pause) { try { [void]$Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown') } catch { Start-Sleep -Seconds 3 } }
    exit 2
}

Write-Host ''
Write-Host "  PackInspect: $FilePath $($parsed -join ' ')" -ForegroundColor Cyan
Write-Host '  ------------------------------------------------------------' -ForegroundColor DarkGray

$exitCode = 1
$output = ''
try {
    # & 调用运算符 + 参数数组：不做字符串拼接，参数里的空格与引号不会被错误拆分。
    # 2>&1 把 stderr 并进同一管道，输出才不会丢。
    # Out-String 把逐行对象合成一整段文本，便于一次写入日志。
    # $ErrorActionPreference 临时放宽：命令写 stderr 不该被当成脚本异常
    $old = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = (& $FilePath @parsed 2>&1 | Out-String)
    } finally {
        $ErrorActionPreference = $old
    }
    # $LASTEXITCODE 在调用外部程序后才有值；为 $null 说明被调用的其实是个 cmdlet
    $exitCode = if ($null -ne $LASTEXITCODE) { $LASTEXITCODE } else { 0 }
} catch {
    # 命令本身起不来（路径不存在、权限不足等）也要留痕，否则界面无从解释
    $output = "PackInspect: 启动失败 - $($_.Exception.Message)"
    $exitCode = 1
}

# 顺序很重要：先写日志、再写状态。父进程以状态文件出现作为"可以读日志"的信号，
# 因此绝不能反过来，否则又会读到不完整的日志。
Save-Log $output
Write-Status $exitCode

# 输出给用户看（命令结束时一次性显示）
if ($output) { Write-Host $output.TrimEnd() }

Write-Host '  ------------------------------------------------------------' -ForegroundColor DarkGray
if ($exitCode -eq 0) {
    Write-Host '  PackInspect: 完成（退出码 0）' -ForegroundColor Green
} else {
    Write-Host "  PackInspect: 失败（退出码 $exitCode）" -ForegroundColor Red
}
Write-Host "  日志: $LogPath" -ForegroundColor DarkGray

if ($Pause) {
    # 默认不走这条分支：等待按键会让后台的等待线程一直挂到窗口关闭，
    # 用户一旦走开就会撞上超时，被误判为执行失败。
    Write-Host ''
    Write-Host '  按任意键关闭此窗口…' -ForegroundColor DarkGray
    try { [void]$Host.UI.RawUI.ReadKey('NoEcho,IncludeKeyDown') } catch { Start-Sleep -Seconds 3 }
} else {
    # 让窗口多留几秒，但**不能在本进程里 Sleep**：本进程一旦 Sleep，
    # PackInspect 的等待线程就要陪着一起等。把"延时关窗"交给独立的分离子进程，
    # 本进程立刻退出。
    Write-Host '  窗口将在 5 秒后自动关闭…' -ForegroundColor DarkGray
    $holdScript = Join-Path ([System.IO.Path]::GetTempPath()) ("packinspect-hold-" + [guid]::NewGuid().ToString('N') + ".ps1")
    $holdBody = 'Start-Sleep -Seconds 5; Remove-Item -LiteralPath $MyInvocation.MyCommand.Path -Force -ErrorAction SilentlyContinue'
    try {
        [System.IO.File]::WriteAllText($holdScript, $holdBody, $utf8NoBom)
        # -NoNewWindow 让它留在同一个控制台里，这样窗口在父进程退出后仍然存在
        Start-Process -FilePath 'powershell.exe' `
            -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $holdScript) `
            -NoNewWindow -ErrorAction Stop | Out-Null
    } catch {
        # 起不来也无妨：窗口直接关掉，日志已经完整落盘
    }
}
