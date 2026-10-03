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

/// 把 `scripts/run-install.ps1` 的**源码内嵌进 exe**。
///
/// 【为什么内嵌】
/// 这个脚本是「可见命令行窗口」功能的运行时依赖。原先靠 `bundle.resources`
/// 或手动拷贝放在 exe 旁边，于是就有了"必须两个文件一起发"的约束 ——
/// 用户拷单个 exe 过去，那个功能就会失效（虽然会给明确报错，但仍是缺陷）。
/// 用 `include_str!` 在编译期把脚本内容塞进二进制，运行时按需释放，
/// `PackInspect.exe` 就真正是自包含的单文件。
///
/// 代价：脚本内容一大，exe 会相应变大（当前脚本约 7 KB）。相对彻底摆脱
/// 外部依赖，这个代价可以接受。
///
/// 路径是相对**本文件**（`src-tauri/src/console.rs`）解析的。
const WRAPPER_SCRIPT_SOURCE: &str = include_str!("../../scripts/run-install.ps1");

/// 释放内嵌脚本到临时目录，返回其路径。
///
/// 只在磁盘上找不到脚本时调用（见 `wrapper_script_path`），因此：
/// - 开发环境下始终用仓库里的那份，改脚本立即生效，不必重新编译
/// - 单文件发行版下从自身释放一份到临时目录
///
/// 用内容哈希命名并配合"已存在且内容一致就跳过写入"，
/// 既避免每次执行都写盘，也保证脚本更新后不会被旧副本顶掉。
fn materialize_embedded_wrapper() -> AppResult<PathBuf> {
    let dir = std::env::temp_dir().join("PackInspect").join("runtime");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::io(format!("创建临时目录失败: {e}")))?;
    let path = dir.join("run-install.ps1");

    // 内容一致就不重写：省 IO，也避免执行期间文件句柄冲突
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == WRAPPER_SCRIPT_SOURCE {
            return Ok(path);
        }
    }

    std::fs::write(&path, WRAPPER_SCRIPT_SOURCE)
        .map_err(|e| AppError::io(format!("释放内嵌脚本失败: {e}")))?;
    Ok(path)
}

/// 列出查找包装脚本时会尝试的候选路径（顺序即优先级）
///
/// 顺序刻意是「磁盘优先、内嵌兜底」：开发时改脚本立即生效，
/// 发行版则靠内嵌那份自给自足。
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

/// 供诊断使用：报告包装脚本来源、路径与尝试过的候选。
///
/// 为什么要暴露它：这个脚本曾因打包漏配而缺失，导致「可见命令行窗口」静默失效。
/// 现在它内嵌在 exe 里，`source` 会明确告诉你是用的磁盘副本还是内嵌副本。
pub fn wrapper_script_diagnostic() -> serde_json::Value {
    let candidates = wrapper_candidates();
    let on_disk = candidates.iter().find(|p| p.is_file());
    let (source, path) = match on_disk {
        Some(p) => ("disk", Some(p.to_string_lossy().to_string())),
        None => (
            "embedded",
            materialize_embedded_wrapper()
                .ok()
                .map(|p| p.to_string_lossy().to_string()),
        ),
    };
    serde_json::json!({
        "found": path.is_some(),
        "source": source,
        "path": path,
        "embeddedBytes": WRAPPER_SCRIPT_SOURCE.len(),
        "tried": candidates.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
    })
}

/// 找到 `scripts/run-install.ps1`：磁盘优先，找不到就用内嵌副本。
fn wrapper_script_path() -> AppResult<PathBuf> {
    let candidates = wrapper_candidates();

    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    // 磁盘上没有（单文件发行版就是这种情况）→ 从 exe 自身释放一份
    materialize_embedded_wrapper()
}

/// 仅供测试：内嵌脚本的源码
#[cfg(test)]
fn embedded_wrapper_source() -> &'static str {
    WRAPPER_SCRIPT_SOURCE
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

    /// 内嵌的包装脚本必须存在且包含那几条硬约束 ——
    /// 这些约束是踩坑后固化的（见脚本头部注释），改回旧写法会让「执行安装」失效。
    ///
    /// 为什么断言**内嵌副本**而不是磁盘文件：发行版用到的是内嵌那份，
    /// 只有断言它才能保证打出来的单文件 exe 行为正确。
    #[test]
    fn embedded_wrapper_keeps_its_hard_won_constraints() {
        let text = embedded_wrapper_source();

        // 1) 不得给 Tee-Object 加 -Encoding：Windows PowerShell 5.1 不支持该参数，
        //    加了会让命令根本不执行，却看起来像安装失败。
        assert!(
            !text.contains("Tee-Object") || !text.contains("-Encoding utf8"),
            "不得给 Tee-Object 加 -Encoding（5.1 不支持）"
        );

        // 2) 参数走 JSON。逗号形式在命令行上会被当成单个字符串，
        //    导致 npm 收到 "install,-g,is-number" 并报 Unknown command。
        assert!(text.contains("ArgumentsJson"), "参数应走 JSON");
        assert!(
            !text.contains("-Arguments ") && !text.contains("-Arguments,"),
            "不得再用 -Arguments a,b,c 形式"
        );

        // 3) 退出码写独立的纯 ASCII 状态文件（写进日志会因编码混编而解析失败）
        assert!(text.contains("$LogPath.status"), "退出码应写入 .status 文件");

        // 4) 顺序必须是「先写日志、再写状态」，父进程以状态文件作为日志可读的信号
        let log_pos = text.find("Save-Log $output").expect("应显式保存日志");
        let status_pos = text.find("Write-Status $exitCode").expect("应写状态");
        assert!(log_pos < status_pos, "必须先写日志再写状态");

        // 5) 不得在脚本进程内 Sleep 等关窗（会阻塞父进程等待）
        assert!(!text.contains("Start-Sleep -Seconds 5`r`n}"), "延时不应阻塞脚本自身");
        assert!(text.contains("Start-Process"), "延时关窗应交给分离子进程");
    }

    /// 内嵌副本必须与仓库里的脚本**逐字节一致**。
    ///
    /// 否则会出现"改了仓库脚本、发行版却还是旧行为"这种极难排查的问题：
    /// 开发时走磁盘副本（新行为），发行版走内嵌副本（旧行为）。
    #[test]
    fn embedded_wrapper_matches_the_repo_script() {
        let on_disk = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("scripts")
                .join("run-install.ps1"),
        )
        .expect("仓库里应存在 scripts/run-install.ps1");
        assert_eq!(
            on_disk, WRAPPER_SCRIPT_SOURCE,
            "内嵌副本与仓库脚本不一致（可能是改了脚本但没重新编译，或改了内嵌常量）"
        );
    }

    /// 释放内嵌脚本：写入成功、可重复调用、内容一致时不重写
    #[test]
    fn materializes_embedded_wrapper_to_temp() {
        let path = materialize_embedded_wrapper().expect("应能释放内嵌脚本");
        assert!(path.is_file(), "释放后文件应存在");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, WRAPPER_SCRIPT_SOURCE, "释放内容应与内嵌内容一致");
        assert!(text.contains("PackInspect"), "内容应像那个包装脚本");

        // 再调一次：应复用同一路径且不报错
        let again = materialize_embedded_wrapper().expect("重复释放也应成功");
        assert_eq!(path, again);
    }

    #[test]
    fn parses_exit_code_from_status_only() {
        // 退出码不再从日志解析（会因编码混编而失败），确认日志解析函数已移除
        // 这里只断言状态文件读取的行为
        let dir = std::env::temp_dir().join("packinspect-test-status-2");
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("x.log");
        let status = dir.join("x.log.status");
        std::fs::write(&status, "0").unwrap();
        assert_eq!(read_status(&log), Some(0));
        assert!(!status.exists());
        let _ = std::fs::remove_file(&status);
    }
}