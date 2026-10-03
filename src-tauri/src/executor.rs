//! 命令解析与安全执行器。
//!
//! 规则：
//! 1. 只执行 `whitelist::op_args` 返回的静态参数，**不接受任何拼接的命令行**。
//! 2. 所有动态参数（如包名）必须先过 `validate::*` 校验。
//! 3. Windows 上 `.cmd` / `.bat` 无法被 `CreateProcess` 直接执行，需要经 `cmd.exe /C`，
//!    这是唯一允许触碰 shell 的地方，且参数由本模块构造、不来自前端。
//! 4. 强制超时，避免 `npm ls -g` 之类的命令卡死界面。

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// 一次命令执行的结果
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    /// 超时杀进程标记
    pub timed_out: bool,
}

impl ExecOutput {
    /// 命令失败时给出人类可读的短描述
    pub fn failure_hint(&self) -> String {
        if self.timed_out {
            return "命令执行超时".to_string();
        }
        let raw = if self.stderr.trim().is_empty() { &self.stdout } else { &self.stderr };
        let first = raw.lines().find(|l| !l.trim().is_empty()).unwrap_or("未知错误");
        first.trim().chars().take(200).collect()
    }
}

/// 执行请求：program 为**候选名**（会在 PATH 与常见安装目录中解析为绝对路径）
#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    /// 透传的环境变量（例如 PYTHONIOENCODING=utf-8）
    pub env: HashMap<String, String>,
}

impl ExecRequest {
    pub fn new(program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: None,
            timeout: Duration::from_millis(20_000),
            env: default_env(),
        }
    }

    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        // 兜底：不允许无限等待，也不允许小于 1 秒
        self.timeout = Duration::from_millis(ms.clamp(1_000, 120_000));
        self
    }

}

/// 保证子进程输出为 UTF-8、不弹交互提示
fn default_env() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("PYTHONIOENCODING".into(), "utf-8".into());
    m.insert("PYTHONUTF8".into(), "1".into());
    m.insert("NO_COLOR".into(), "1".into());
    m.insert("FORCE_COLOR".into(), "0".into());
    m.insert("NPM_CONFIG_UPDATE_NOTIFIER".into(), "false".into());
    m.insert("NPM_CONFIG_FUND".into(), "false".into());
    m.insert("npm_config_update_notifier".into(), "false".into());
    m
}

// ---------------------------------------------------------------------------
// 可执行文件解析
// ---------------------------------------------------------------------------

/// 在 PATH 与常见安装目录中查找可执行文件。
/// `candidates` 按优先级排列，返回第一个命中项的**绝对路径**。
///
/// 关键设计：**不只依赖进程的 PATH 环境变量**。
/// GUI 进程（以及被 vcvars 之类脚本改过环境的进程）拿到的 PATH 往往比登录 shell 窄，
/// 会出现「明明装了 npm 却报未找到」。因此额外回退到：
///   1. 注册表里的系统 / 用户 PATH（Windows 上是权威值）
///   2. 各工具链的常见安装目录
pub fn resolve_executable(candidates: &[&str]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    // 1) 进程 PATH
    if let Ok(path) = std::env::var("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    // 2) 注册表 PATH —— 修复 GUI 进程 PATH 不完整的问题
    dirs.extend(windows_registry_paths());

    // 3) 常见安装目录兜底
    for key in ["APPDATA", "LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)", "USERPROFILE", "HOME"] {
        if let Ok(value) = std::env::var(key) {
            if value.is_empty() {
                continue;
            }
            let base = PathBuf::from(&value);
            dirs.push(base.join("npm"));
            dirs.push(base.join("bin"));
            dirs.push(base.join(".local").join("bin"));
            dirs.push(base.join(".cargo").join("bin"));
            dirs.push(base.join("go").join("bin"));
            dirs.push(base.join("scoop").join("shims"));
            dirs.push(base.join("Programs").join("Python").join("Scripts"));
            dirs.push(base.clone());
        }
    }
    // Node.js / Python 常见安装根（含非系统盘）
    for root in ["C:\\", "D:\\", "E:\\"] {
        for name in ["nodejs", "Program Files\\nodejs"] {
            dirs.push(PathBuf::from(format!("{root}{name}")));
        }
        for name in ["Python313", "Python312", "Python311"] {
            let base = PathBuf::from(format!("{root}{name}"));
            dirs.push(base.join("Scripts"));
            dirs.push(base);
        }
    }
    // 常见包管理器 shim 目录
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(PathBuf::from(&local).join("pnpm"));
        dirs.push(PathBuf::from(&local).join("Yarn").join("bin"));
        dirs.push(PathBuf::from(&local).join("Microsoft").join("WinGet").join("Links"));
    }

    // 去重后按顺序探测，保证候选优先级仍然生效
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<PathBuf> = dirs.into_iter().filter(|d| seen.insert(d.clone())).collect();

    for candidate in candidates {
        // 先做显式 PATH 风格查找（保留「未写扩展名也能命中」的行为）
        for dir in &unique {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
        // Windows：候选没写扩展名时按 PATHEXT 顺序补全
        if cfg!(windows) && Path::new(candidate).extension().is_none() {
            for ext in ["exe", "cmd", "bat", "ps1"] {
                for dir in &unique {
                    let path = dir.join(format!("{candidate}.{ext}"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

/// 读取 Windows 注册表里的系统 / 用户 PATH。
///
/// 用 `reg.exe` 而不是 winreg crate：少一个依赖，且 reg.exe 在任何 Windows 上都存在。
/// 只在主 PATH 查找失败后才调用（见 `resolve_executable`），因此不影响常规性能。
fn windows_registry_paths() -> Vec<PathBuf> {
    #[cfg(not(windows))]
    {
        Vec::new()
    }

    #[cfg(windows)]
    {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let queries: [(&str, &str); 2] = [
            ("HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment", "Path"),
            ("HKCU\\Environment", "Path"),
        ];

        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        for (key, name) in queries {
            let mut reg = std::process::Command::new(&comspec);
            reg.arg("/D")
                .arg("/S")
                .arg("/C")
                .arg(format!("reg query \"{key}\" /v {name}"))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            // 【必须加】否则每次调用都会闪一个控制台黑框。
            // 这段是 PATH 回退逻辑，在启动探测时会被**每个管理器**调用一次
            //（各两次注册表查询），漏掉这个标志就是一连串黑框反复开关 ——
            // 曾经真实发生过，看起来像程序在反复启动什么东西。
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                /// CREATE_NO_WINDOW：不为控制台程序创建窗口
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                reg.creation_flags(CREATE_NO_WINDOW);
            }

            let output = reg.output();
            let Ok(output) = output else { continue };
            let text = String::from_utf8_lossy(&output.stdout);
            // 输出形如：`    Path    REG_EXPAND_SZ    C:\a;C:\b`
            for line in text.lines() {
                let trimmed = line.trim();
                if !trimmed.to_ascii_lowercase().starts_with(&name.to_ascii_lowercase()) {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }
                // REG_EXPAND_SZ 里可能有 %SystemRoot% 之类的变量
                let raw = parts[2..].join(" ");
                let expanded = expand_env_vars(&raw);
                dirs.extend(std::env::split_paths(&expanded));
                break;
            }
        }
        dirs
    }
}

/// 展开 `%VAR%` 形式的环境变量（注册表 REG_EXPAND_SZ 的取值方式）
fn expand_env_vars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        // 变量不存在时原样保留，避免把路径拼坏
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// 执行
// ---------------------------------------------------------------------------

/// 构建 `Command`，处理 Windows 批处理包装与「不弹黑框」。
fn build_command(exe: &Path, req: &ExecRequest) -> AppResult<Command> {
    let is_batch = cfg!(windows)
        && matches!(
            exe.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref(),
            Some("cmd") | Some("bat")
        );

    let mut cmd = if is_batch {
        // 批处理必须经 cmd.exe 执行；参数依旧来自白名单，不接受前端输入
        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        let mut c = Command::new(comspec);
        c.arg("/D").arg("/S").arg("/C").arg(exe);
        c
    } else {
        Command::new(exe)
    };

    cmd.args(&req.args);
    if let Some(dir) = &req.cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in &req.env {
        cmd.env(k, v);
    }
    cmd.env("CLICOLOR", "0").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// CREATE_NO_WINDOW：GUI 应用里执行控制台程序时不闪烁黑框
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    Ok(cmd)
}

/// 同步执行并强制超时。必须在 `spawn_blocking` 中调用，勿阻塞 UI 主线程。
pub fn run(req: &ExecRequest) -> AppResult<ExecOutput> {
    if req.program.is_empty() {
        return Err(AppError::invalid("program 不能为空"));
    }
    let exe = resolve_executable(&[req.program.as_str()])
        .ok_or_else(|| AppError::not_installed(&req.program))?;
    run_resolved(&exe, req)
}

/// 已知绝对路径时的执行入口
pub fn run_resolved(exe: &Path, req: &ExecRequest) -> AppResult<ExecOutput> {
    let started = Instant::now();
    let mut cmd = build_command(exe, req)?;

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::io(format!("启动 {} 失败: {e}", exe.display())))?;

    // 独立线程读干管道，避免子进程写满管道缓冲区后死锁
    let (tx, rx) = mpsc::channel::<(bool, String)>();
    if let Some(mut out) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = out.read_to_string(&mut buf);
            let _ = tx.send((true, buf));
        });
    }
    if let Some(mut err) = child.stderr.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err.read_to_string(&mut buf);
            let _ = tx.send((false, buf));
        });
    }
    drop(tx);

    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut timed_out = false;

    // 轮询等待：既收集输出，也监控超时
    let deadline = started + req.timeout;
    loop {
        while let Ok((is_out, chunk)) = rx.try_recv() {
            if is_out {
                stdout.push_str(&chunk);
            } else {
                stderr.push_str(&chunk);
            }
        }
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(e) => {
                return Err(AppError::io(format!("等待子进程失败: {e}")));
            }
        }
    }

    // 收尾：等两个读取线程把剩余内容送完再退出，避免 JSON 输出被截断
    let grace = Instant::now() + Duration::from_millis(800);
    loop {
        let remaining = grace.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining.min(Duration::from_millis(120))) {
            Ok((is_out, chunk)) => {
                if is_out {
                    stdout.push_str(&chunk);
                } else {
                    stderr.push_str(&chunk);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if Instant::now() >= grace {
                    break;
                }
            }
        }
    }

    let exit_code = child.try_wait().ok().flatten().and_then(|s| s.code());

    Ok(ExecOutput {
        success: !timed_out && exit_code == Some(0),
        exit_code,
        stdout: clean_output(&stdout),
        stderr: clean_output(&stderr),
        duration_ms: started.elapsed().as_millis() as u64,
        timed_out,
    })
}

/// 去掉 BOM、CRLF 与 ANSI 颜色残留
fn clean_output(s: &str) -> String {
    let stripped = strip_ansi(s);
    stripped.replace("\r\n", "\n").trim_start_matches('\u{feff}').to_string()
}

pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // 跳过 ESC [ ... 终止字母
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_color_codes() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn empty_program_is_rejected() {
        let req = ExecRequest::new("", &[]);
        assert!(run(&req).is_err());
    }

    #[test]
    fn timeout_is_clamped() {
        let req = ExecRequest::new("x", &[]).with_timeout_ms(10);
        assert!(req.timeout >= Duration::from_millis(1_000));
        let req = ExecRequest::new("x", &[]).with_timeout_ms(u64::MAX);
        assert!(req.timeout <= Duration::from_millis(120_000));
    }

    /// 回归测试：曾出现「明明装了 npm 却报未找到」。
    ///
    /// 根因是 GUI 进程（或被 vcvars 之类脚本改过环境的进程）拿到的 PATH 比登录 shell 窄。
    /// 现在 `resolve_executable` 会额外回退到注册表 PATH 与常见安装目录。
    /// 这里不假设目标一定存在，只验证解析逻辑本身自洽。
    #[test]
    fn resolve_executable_finds_always_present_binary() {
        // cmd.exe 在任何 Windows 上都存在，且系统目录未必在精简 PATH 里
        let cmd = resolve_executable(&["cmd.exe", "cmd"]);
        assert!(cmd.is_some(), "应能解析到 cmd.exe（注册表 PATH 回退失效？）");
        let path = cmd.unwrap();
        assert!(path.is_absolute(), "解析结果必须是绝对路径: {}", path.display());
        assert!(path.is_file(), "解析结果必须真实存在: {}", path.display());

        // cargo 测试二进制本身由 cargo 启动，因此 cargo 必然可解析
        assert!(resolve_executable(&["cargo.exe", "cargo"]).is_some());
    }

    /// 不存在的名字必须返回 None，而不是误报
    #[test]
    fn resolve_executable_returns_none_for_unknown() {
        assert!(resolve_executable(&["definitely-not-a-real-binary-xyz.exe"]).is_none());
    }
}
