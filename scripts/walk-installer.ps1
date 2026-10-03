# 自动走查 NSIS 安装向导并逐页截图。
#
# 目的：把「安装界面」的实际样子留成证据，而不是只靠文字描述。
# 用 Win32 消息（而不是模拟鼠标）驱动按钮点击，避免受窗口位置 / DPI 影响 ——
# 之前用屏幕坐标截图经常抓到别的窗口。
#
# 用法（由开发者在仓库根执行）：
#   pwsh -File scripts\walk-installer.ps1 -Setup <安装程序路径> -OutDir <截图目录>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Setup,
    [Parameter(Mandatory = $true)][string]$OutDir
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;

/// <summary>NSIS 向导的枚举 / 截图 / 点击辅助。</summary>
public class NsisWalker {
    public delegate bool EnumProc(IntPtr h, IntPtr l);

    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr p, EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint msg, IntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr hDlg, int nIDDlgItem);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr h);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    public static string Title(IntPtr h) {
        var t = new StringBuilder(512); GetWindowTextW(h, t, 512); return t.ToString();
    }
    public static string ClassOf(IntPtr h) {
        var c = new StringBuilder(128); GetClassNameW(h, c, 128); return c.ToString();
    }

    /// <summary>标题含 "PackInspect" 的对话框（安装向导 / 语言选择）</summary>
    public static IntPtr Dialog(string mustContain) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            if (ClassOf(h) != "#32770") return true;
            if (Title(h).Contains(mustContain)) { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static IntPtr FindButton(IntPtr parent, string text) {
        IntPtr found = IntPtr.Zero;
        EnumChildWindows(parent, (h, l) => {
            if (ClassOf(h) != "Button") return true;
            if (Title(h) == text) { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    /// <summary>列出所有子控件文本，用于判断当前是哪一页</summary>
    public static List<string> Texts(IntPtr parent) {
        var list = new List<string>();
        EnumChildWindows(parent, (h, l) => {
            var s = Title(h);
            if (s.Length > 0) list.Add(ClassOf(h) + ": " + s);
            return true;
        }, IntPtr.Zero);
        return list;
    }

    /// <summary>用 PrintWindow 抓完整窗口（不依赖窗口是否在屏幕可见区域内）</summary>
    public static int GrabTo(IntPtr h, string path) {
        RECT r; GetWindowRect(h, out r);
        int w = r.Right - r.Left, ht = r.Bottom - r.Top;
        if (w <= 0 || ht <= 0) return 0;
        var bmp = new System.Drawing.Bitmap(w, ht);
        using (var g = System.Drawing.Graphics.FromImage(bmp)) {
            IntPtr hdc = g.GetHdc();
            PrintWindow(h, hdc, 0);
            g.ReleaseHdc(hdc);
        }
        bmp.Save(path, System.Drawing.Imaging.ImageFormat.Png);
        bmp.Dispose();
        return w;
    }

    /// <summary>BM_CLICK —— 比模拟鼠标可靠，不受窗口位置影响</summary>
    public static void Click(IntPtr b) { SendMessageW(b, 0x00F5, IntPtr.Zero, IntPtr.Zero); }
}
'@

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

function Get-Wizard { [NsisWalker]::Dialog('PackInspect') }

function Save-Page {
    param([string]$Name)
    $w = Get-Wizard
    if ($w -eq [IntPtr]::Zero) { Write-Host "    （向导已关闭）"; return }
    $path = Join-Path $OutDir $Name
    $width = [NsisWalker]::GrabTo($w, $path)
    Write-Host "    已保存 $Name（宽 ${width}px）"
    foreach ($t in [NsisWalker]::Texts($w)) {
        if ($t -like 'Static*' -and $t.Length -gt 9) { Write-Host "      $t" }
    }
}

function Click-Button {
    param([string]$Text, [int]$WaitSeconds = 2)
    $w = Get-Wizard
    if ($w -eq [IntPtr]::Zero) { return $false }
    $b = [NsisWalker]::FindButton($w, $Text)
    if ($b -eq [IntPtr]::Zero) { Write-Host "    （找不到按钮「$Text」）"; return $false }
    [NsisWalker]::Click($b)
    Write-Host "    → 点击「$Text」"
    Start-Sleep -Seconds $WaitSeconds
    return $true
}

Write-Host "启动安装程序: $Setup"
$proc = Start-Process -FilePath $Setup -PassThru
Start-Sleep -Seconds 5

# 1) 语言选择页（displayLanguageSelector = true 时出现）
Write-Host "`n=== 语言选择 ==="
$lang = [NsisWalker]::Dialog('Installer Language')
if ($lang -ne [IntPtr]::Zero) {
    Save-Page '0-language.png'
    $ok = [NsisWalker]::FindButton($lang, 'OK')
    if ($ok -ne [IntPtr]::Zero) { [NsisWalker]::Click($ok); Write-Host "    → 点击「OK」"; Start-Sleep -Seconds 3 }
} else {
    Write-Host "    （未出现语言选择页）"
}

# 2) 欢迎页
Write-Host "`n=== 欢迎页 ==="
Save-Page '1-welcome.png'
if (-not (Click-Button '下一步(&N) >' 3)) { Write-Host "    无法继续，可能停在维护页"; }

# 3) 后续页：确认 / 进度 / 完成；逐页截图直到向导关闭
#
# 按钮标签在不同语言与不同阶段都不一样，因此按**优先级**逐个尝试 ——
# 只匹配一个固定文案会在"已安装 / 维护"分支上卡住（实测踩过）。
$advanceLabels = @(
    '安装(&I)',
    '安装(I)',
    '安装(&I) >',
    '下一步(&N) >',
    '完成(&F)',
    '完成(F)'
)
$step = 2
while ($step -le 6) {
    $w = Get-Wizard
    if ($w -eq [IntPtr]::Zero) { Write-Host "`n（向导已关闭，流程结束）"; break }
    Write-Host "`n=== 第 $step 页 ==="
    Save-Page "$step-page.png"

    $advanced = $false
    foreach ($label in $advanceLabels) {
        if (Click-Button $label 4) { $advanced = $true; break }
    }
    if (-not $advanced) { Write-Host "    （没有可推进的按钮，停止）"; break }
    $step++
}

# 若安装已完成，向导进程应已退出
Start-Sleep -Seconds 2
if (-not $proc.HasExited) {
    Write-Host "`n（安装程序仍在运行，主动关闭以免影响后续测试）"
    $w = Get-Wizard
    if ($w -ne [IntPtr]::Zero) { [NsisWalker]::Click([NsisWalker]::FindButton($w, '取消(&C)')) }
}
Write-Host "`n截图输出目录: $OutDir"
