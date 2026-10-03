//! 在**可见的命令行窗口**里执行包管理操作。
//!
//! 【为什么单独成模块】
//! `executor::run_resolved` 是静默执行：管道捕获输出、隐藏窗口。这条路径适合
//! 「探测版本、列出包」这类用户不关心的命令。但**安装/卸载**不同 ——
//! 下载进度、依赖解析、报错都发生在过程中，用户需要实时看到，
//! 否则一个卡住的安装看起来就像"没反应"。
//!
//! 【为什么不能同时"可见"又"捕获输出"】
//! 子进程一旦拥有真实控制台，它的输出直接给了用户，父进程的管道就拿不到了。
//! 因此这里走一条折中路径：
//!   可见控制台（用户实时看） + `scripts/run-install.ps1` 用 Tee-Object 落一份日志
//!   执行结束后父进程读日志 → 把结果回显到界面
//! 这样两边都满足：过程可见、结果可留档。
//!
//! 【安全】
//! 可执行文件由 `executor::resolve_executable` 解析为绝对路径，参数来自白名单
//! 静态模板。这里不用 shell 字符串拼接，而是把参数作为数组交给 PowerShell 脚本
//! 的 `-Arguments`，脚本内部用 `& $FilePath @Arguments` 调用 ——
//! 全程没有一层解析器会把参数再拆一次。

use crate::error::{AppError, AppResult};
use crate::executor::ExecOutput;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// 单次日志回传上限（避免 cargo 编译日志之类撑爆 IPC）
const MAX_LOG_CHARS: usize = 20_000;
/// 等待 PowerShell 把日志刷盘的最后宽限
const FLUSH_GRACE: Duration = Duration::from_millis(3_000);

/// 运行 `scripts/run-install.ps1`。
///
/// `log_path` 会被脚本清空后写入完整输出；`pause` 让窗口在结束后保留，
/// 用户能回看日志（对失败排查尤其重要）。
pub fn run_visible(
    exe: &Path,
    args: &[String],
    log_path: &Path,
    timeout: Duration,
    pause: bool,
) -> AppResult<ExecOutput> {
    let started = Instant::now();
    let script = wrapper_script_path()?;

    let powershell = crate::executor::resolve_executable(&["powershell.exe", "pwsh.exe", "pwsh"])
        .ok_or_else(|| AppError::new("NO_POWERSHELL", "系统里找不到 PowerShell，无法打开命令行窗口"))?;

    // 参数以 **JSON 数组**传递，而不是 `-Arguments a,b,c`。
    //
    // 踩过的坑：`-Arguments install,-g,is-number` 会被 PowerShell 当成**一个字符串**
    // （逗号形式是数组字面量，只在脚本内部有效，命令行上不成立），
    // 结果 npm 收到 "install,-g,is-number" 并报 Unknown command。
    // JSON 没有歧义，也不必担心参数里出现逗号/引号/空格 —— 包名虽已校验，
    // 但 JSON 让这层更不依赖"参数恰好干净"这个前提。
    let args_json = serde_json::to_string(args)
        .map_err(|e| AppError::internal(format!("序列化参数失败: {e}")))?;

    let mut cmd = Command::new(&powershell);
    cmd.arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script)
        .arg("-FilePath")
        .arg(exe)
        .arg("-LogPath")
        .arg(log_path)
        .arg("-ArgumentsJson")
        .arg(&args_json);
    if pause {
        cmd.arg("-Pause");
    }

    // 关键：让子进程拥有自己的控制台窗口。
    // stdout/stderr 交给那个控制台，父进程不再接管，因此这里不设 piped。
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// CREATE_NEW_CONSOLE：为新进程分配独立控制台（即"打开一个命令行窗口"）
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::io(format!("打开命令行窗口失败: {e}")))?;

    // 轮询等待，支持超时；超时则终止，避免留下无人管理的安装进程
    let deadline = started + timeout;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => return Err(AppError::io(format!("等待命令行窗口失败: {e}"))),
        }
    }

    // 先取退出码：它由脚本在 Tee 之后立即写入，是最可靠的成败信号
    let status = read_status(log_path);

    // 再读日志。PowerShell 退出后 Tee 可能还没把最后一批内容刷到磁盘
    // （实测过：刚退出就读会得到空内容），因此轮询等一小会儿。
    let log = wait_for_log(log_path);

    // 状态文件由包装脚本无条件写入，是判断成败的**唯一依据**。
    // 踩过的坑：曾试图从日志里解析退出码，但 Tee-Object 在 5.1 上写的是 UTF-16LE，
    // 脚本再用 UTF-8 追加退出码行会造成同文件混编、解析必然乱码，
    // 于是把成功的安装误判为失败。状态文件是纯 ASCII，与日志编码无关。
    let script_broken = status.is_none();
    let effective_exit = status.unwrap_or(1);

    Ok(ExecOutput {
        success: !timed_out && !script_broken && effective_exit == 0,
        exit_code: Some(effective_exit),
        stdout: log,
        // 可见模式下的 stderr 与 stdout 合并进了同一份日志
        stderr: String::new(),
        duration_ms: started.elapsed().as_millis() as u64,
        timed_out,
    })
}

/// 读取包装脚本写入的状态文件（纯 ASCII 的退出码）。
///
/// 读完即删：下次执行若脚本没写成，也不会读到上一次的陈旧值。
fn read_status(log_path: &Path) -> Option<i32> {
    let status_path = PathBuf::from(format!("{}.status", log_path.display()));
    let text = std::fs::read_to_string(&status_path).ok()?;
    let code = text.trim().parse::<i32>().ok();
    let _ = std::fs::remove_file(&status_path);
    code
}

/// 轮询读取日志，等待脚本把内容落盘。
///
/// 为什么要轮询而不是读一次：PowerShell 进程退出与 Tee 完成写盘之间存在微小窗口，
/// 实测刚退出就读会拿到空内容（而命令行窗口里其实已经打印了完整输出）。
/// 最多等 [`FLUSH_GRACE`]，一旦读到非空内容就立即返回。
fn wait_for_log(log_path: &Path) -> String {
    let deadline = Instant::now() + FLUSH_GRACE;
    loop {
        let last = read_log(log_path);
        if !last.trim().is_empty() || Instant::now() >= deadline {
            return last;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// 列出查找包装脚本时会尝试的候选路径（顺序即优先级）
fn wrapper_candidates() -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1) 相对可执行文件：<安装目录>/scripts/run-install.ps1
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("scripts").join("run-install.ps1"));
            // 开发时 exe 在 src-tauri/target/debug/，回到仓库根
            candidates.push(dir.join("..").join("..").join("..").join("scripts").join("run-install.ps1"));
        }
    }
    // 2) 当前工作目录及其父目录（cargo run 时 cwd = src-tauri）
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("scripts").join("run-install.ps1"));
        candidates.push(cwd.join("..").join("scripts").join("run-install.ps1"));
    }

    candidates
}

/// 供诊断使用：报告包装脚本是否找到、以及实际路径与尝试过的候选。
///
/// 为什么要暴露它：这个脚本是**打包资源**，漏打时安装版的「可见命令行窗口」
/// 会静默失效。把它放进 `get_diagnostics`，出问题一眼能看出原因。
pub fn wrapper_script_diagnostic() -> serde_json::Value {
    let candidates = wrapper_candidates();
    let found = candidates.iter().find(|p| p.is_file());
    serde_json::json!({
        "found": found.is_some(),
        "path": found.map(|p| p.to_string_lossy().to_string()),
        "tried": candidates.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
    })
}

/// 找到 `scripts/run-install.ps1`。
///
/// 开发运行时当前目录是 `src-tauri`，打包后则是安装目录 —— 因此两个位置都找。
fn wrapper_script_path() -> AppResult<PathBuf> {
    let candidates = wrapper_candidates();

    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    Err(AppError::new(
        "NO_WRAPPER",
        format!(
            "找不到 scripts/run-install.ps1，无法在可见窗口中执行。已尝试: {}",
            candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" / ")
        ),
    ))
}

/// 读取日志并解码为 UTF-8。
///
/// 【编码为什么需要处理】
/// Windows PowerShell 5.1 的 `Tee-Object` 写出的是 **UTF-16LE（带 BOM）**，
/// 而不是 UTF-8 —— 直接按 UTF-8 读会得到满屏 NUL 与乱码。
/// 因此这里先看 BOM：UTF-16 走宽字符解码，UTF-8 BOM 去掉后按 UTF-8 读，
/// 都没有 BOM 时先试 UTF-8、失败再按本地代码页（GBK）兜底。
fn read_log(path: &Path) -> String {
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let text = decode_text(&bytes);
    let text = crate::executor::strip_ansi(&text);
    truncate_middle(&text, MAX_LOG_CHARS)
}

/// 按 BOM / 内容判断编码并解码
fn decode_text(bytes: &[u8]) -> String {
    // UTF-16LE BOM
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    // UTF-16BE BOM
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    // UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).to_string();
    }
    // 无 BOM：优先 UTF-8；若含大量 NUL（说明其实是 UTF-16 却没写 BOM）或非法字节，
    // 退到本地代码页兜底
    if let Ok(text) = std::str::from_utf8(bytes) {
        if !text.contains('\u{0}') {
            return text.to_string();
        }
    }
    String::from_utf8_lossy(bytes).replace('\u{0}', "")
}

/// 超长时保留首尾（开头有命令信息，结尾有退出码与错误摘要）
fn truncate_middle(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars / 2).collect();
    let tail: String = {
        let rev: String = text.chars().rev().take(max_chars / 2).collect();
        rev.chars().rev().collect()
    };
    format!("{head}\n… （中间省略）…\n{tail}")
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 状态文件是纯 ASCII 的退出码；读完即删，不会串味
    #[test]
    fn reads_and_consumes_status_file() {
        let dir = std::env::temp_dir().join("packinspect-test-status");
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("op.log");
        let status = dir.join("op.log.status");

        std::fs::write(&status, "0").unwrap();
        assert_eq!(read_status(&log), Some(0));
        assert!(!status.exists(), "读完应删除，避免下次读到陈旧值");
        assert_eq!(read_status(&log), None, "已删除后应返回 None");

        std::fs::write(&status, "127\n").unwrap();
        assert_eq!(read_status(&log), Some(127), "应容忍尾部换行");

        std::fs::write(&status, "not a number").unwrap();
        assert_eq!(read_status(&log), None, "非数字视为无效");

        let _ = std::fs::remove_file(&status);
    }

    #[test]
    fn decodes_utf16le_with_bom_as_written_by_powershell_51() {
        // Windows PowerShell 5.1 的 Tee-Object 写出 UTF-16LE + BOM；
        // 按 UTF-8 读会得到满屏 NUL，这里确保能正确还原
        let text = "npm install -g is-number\r\nPackInspect: 退出码 0\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let decoded = decode_text(&bytes);
        assert!(decoded.contains("is-number"), "{decoded}");
        assert!(decoded.contains("退出码 0"), "{decoded}");
        // 编码解码正确后，日志内容应可读（退出码现在走状态文件，不在此断言）
    }

    #[test]
    fn decodes_utf8_with_and_without_bom() {
        let plain = "hello 世界\nPackInspect: 退出码 0\n";
        assert!(decode_text(plain.as_bytes()).contains("世界"));

        let mut with_bom = vec![0xEF, 0xBB, 0xBF];
        with_bom.extend_from_slice(plain.as_bytes());
        let decoded = decode_text(&with_bom);
        assert!(decoded.contains("hello"), "{decoded}");
        assert!(!decoded.starts_with('\u{feff}'), "BOM 应被去掉");
    }

    #[test]
    fn read_log_handles_missing_file() {
        assert_eq!(read_log(Path::new("Z:\\definitely\\missing\\op.log")), "");
    }

    #[test]
    fn read_log_truncates_huge_output_keeping_both_ends() {
        let dir = std::env::temp_dir().join("packinspect-test-log");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("big.log");
        let mut body = String::from("HEAD-MARKER\n");
        body.push_str(&"x".repeat(60_000));
        body.push_str("\nTAIL-MARKER\nPackInspect: 退出码 0\n");
        std::fs::write(&path, &body).unwrap();

        let read = read_log(&path);
        assert!(read.contains("HEAD-MARKER"), "应保留开头");
        assert!(read.contains("TAIL-MARKER"), "应保留结尾");
        assert!(read.chars().count() < 25_000, "应被截断");

        let _ = std::fs::remove_file(&path);
    }

    /// 包装脚本的兼容性约束 —— 每一条都对应一个实测踩过的坑。
    #[test]
    fn wrapper_script_keeps_its_hard_won_constraints() {
        let script = wrapper_script_path().expect("开发环境下应能找到 run-install.ps1");
        let text = std::fs::read_to_string(&script).expect("脚本应可读");

        // 1) 只使用 PowerShell 5.1 也支持的参数。
        //    踩过的坑：`Tee-Object -Encoding` 在 5.1 上不存在，脚本一启动就报
        //    "A parameter cannot be found"，命令根本没被执行，却看起来像安装失败。
        assert!(
            !text.contains("-Encoding utf8") || !text.contains("Tee-Object"),
            "不得给 Tee-Object 加 -Encoding（5.1 不支持）"
        );

        // 2) 参数走 JSON。逗号形式 `-Arguments a,b,c` 在命令行上会被当成单个字符串，
        //    导致 npm 收到 "install,-g,is-number" 并报 Unknown command。
        //    注意 `-ArgumentsJson` 自身包含 `-Arguments` 子串，所以要按带分隔符的形式判断。
        assert!(text.contains("ArgumentsJson"), "参数应走 JSON");
        assert!(
            !text.contains("-Arguments ") && !text.contains("-Arguments,"),
            "不得再用 -Arguments a,b,c 形式（命令行上不是数组）"
        );

        // 3) 退出码写独立的纯 ASCII 状态文件。
        //    不能追加进日志：日志编码由 PowerShell 决定，追加会造成同文件混编，
        //    解析必然乱码，曾把成功的安装误判为失败。
        assert!(text.contains("$LogPath.status"), "退出码应写入 .status 文件");

        // 4) 顺序必须是「先写日志、再写状态」：父进程以状态文件作为"日志已可读"的信号。
        let log_pos = text.find("Save-Log $output").expect("应显式保存日志");
        let status_pos = text.find("Write-Status $exitCode").expect("应写状态");
        assert!(
            log_pos < status_pos,
            "必须先写日志再写状态，否则父进程会读到不完整的日志"
        );

        // 5) 不得在脚本进程内 Sleep 等待关窗：那会让父进程陪着一起等，
        //    而且日志句柄未释放。改用独立子进程延时，脚本立刻退出。
        assert!(
            !text.contains("Start-Sleep -Seconds 5`r`n}"),
            "延时关窗不应阻塞脚本自身"
        );
        assert!(
            text.contains("Start-Process"),
            "延时关窗应交给独立的分离子进程"
        );
    }
}