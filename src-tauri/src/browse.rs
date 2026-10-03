//! 在包仓库里搜索「可安装的新包」。
//!
//! 【安全与可用性取舍】
//! - 只查询各生态**已知的搜索端点**，地址由本模块的模板拼接，不接受前端传入 URL；
//! - 关键词先过 `validate::search_query`（长度 + 字符集），再按生态规则 URL 编码；
//! - 联网失败、超时、返回格式变化都不抛错，而是通过 `BrowseResult` 把
//!   「真的没有」「网络失败」「该生态不支持关键词搜索」三种情况区分开 ——
//!   只返回空数组会让界面一律显示「没有找到匹配的包」，这是最误导用户的做法。
//!
//! 各生态的实际能力（全部实测过）：
//!
//! | 生态 | 端点 | 关键词搜索 |
//! |---|---|---|
//! | npm / pnpm / yarn | registry.npmjs.org/-/v1/search | ✅ |
//! | cargo | crates.io/api/v1/crates | ✅ |
//! | dotnet | azuresearch-usnc.nuget.org/query | ✅ |
//! | composer | packagist.org/search.json | ✅ |
//! | gem | rubygems.org/api/v1/search.json | ✅ |
//! | dart | pub.dev/api/search | ✅ |
//! | winget | 本地 `winget search` | ✅ |
//! | powershellget | PowerShell Gallery OData | ✅（Atom XML） |
//! | pip | pypi.org/pypi/&lt;name&gt;/json | ❌ 改为**精确名查询** |
//!
//! pip 的限制：`pypi.org/search` 是 JS 渲染页面（HTML 里没有任何结果链接，已实测确认），
//! libraries.io 需要 API key。PyPI 官方可编程访问的只有 `/pypi/<name>/json` 精确查询，
//! 因此这里改为「按精确名查询 + 明确提示用户输入完整包名」。

use crate::error::{AppError, AppResult};
use crate::executor::{self, ExecRequest};
use crate::fsutil;
use crate::models::{BrowseRequest, BrowseResult, InstallPlan, RemotePackage};
use crate::validate;
use serde_json::Value;

/// 搜索类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Npm,
    Crates,
    Nuget,
    Packagist,
    Rubygems,
    Pub,
    PowerShellGallery,
    /// 本地 winget 命令
    WingetCli,
    /// PyPI 精确名查询（无关键词搜索）
    PypiExact,
}

/// 生态 → (URL 模板, 解析类型)
///
/// 模板里 `{q}` 会被替换为编码后的关键词，`{limit}` 为条数。
fn endpoint_for(manager: &str) -> Option<(&'static str, Kind)> {
    match manager {
        "npm" | "pnpm" | "yarn" => {
            Some(("https://registry.npmjs.org/-/v1/search?size={limit}&text={q}", Kind::Npm))
        }
        "cargo" => Some(("https://crates.io/api/v1/crates?per_page={limit}&q={q}", Kind::Crates)),
        "dotnet" => Some((
            "https://azuresearch-usnc.nuget.org/query?take={limit}&q={q}&prerelease=false",
            Kind::Nuget,
        )),
        "composer" => {
            Some(("https://packagist.org/search.json?per_page={limit}&q={q}", Kind::Packagist))
        }
        "gem" => Some(("https://rubygems.org/api/v1/search.json?query={q}", Kind::Rubygems)),
        "dart" => Some(("https://pub.dev/api/search?query={q}", Kind::Pub)),
        "powershellget" => Some((
            // 注意：`$orderby=DownloadCount%20desc` 里的空格必须编码成 %20。
            // 用字面空格会被 curl 直接拒绝（实测：「URL rejected: Malformed input to a URL function」），
            // 表现为该生态永远搜不到任何东西。
            "https://www.powershellgallery.com/api/v2/Search()?$filter=IsLatestVersion&$skip=0&$top={limit}&searchTerm='{q}'&targetFramework=''&includePrerelease=false&$orderby=DownloadCount%20desc&$inlinecount=allpages",
            Kind::PowerShellGallery,
        )),
        "winget" => Some(("winget-cli", Kind::WingetCli)),
        // PyPI：没有关键词搜索，按精确名查
        "pip" => Some(("https://pypi.org/pypi/{q}/json", Kind::PypiExact)),
        _ => None,
    }
}

/// 该生态的搜索能力说明（展示给用户，避免把「能力限制」误解为「搜不到」）
fn capability_note(manager: &str) -> Option<String> {
    match manager {
        "pip" => Some(
            "PyPI 没有可用的关键词搜索接口（其搜索页由 JS 渲染、无公开 JSON API），\
             因此这里按「精确包名」查询。请输入完整包名，例如 requests、numpy。"
                .into(),
        ),
        _ => None,
    }
}

/// 把关键词编码进 URL（只允许安全字符，其余 percent-encode）
fn encode_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len() * 3);
    for byte in query.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// 通过 curl 拉取 URL 并返回文本。
///
/// 为什么用 curl.exe 而不是引入 HTTP 客户端 crate：
/// Windows 10+ 自带 curl.exe，省掉 reqwest + tokio 一大串依赖（构建时间与体积都受益）。
/// 这同样属于「外部命令调用」，因此参数是**静态标志 + 本模块构造的 URL**，不来自前端。
fn http_get(url: &str, timeout_ms: u64) -> AppResult<String> {
    let curl = executor::resolve_executable(&["curl.exe", "curl"])
        .ok_or_else(|| AppError::new("NO_CURL", "系统中没有找到 curl，无法查询在线仓库"))?;

    let seconds = (timeout_ms / 1000).clamp(3, 30);
    let args = vec![
        "-sS".to_string(),
        "-L".to_string(),
        "--max-time".to_string(),
        seconds.to_string(),
        // 失败时也要把 HTTP 状态码带出来，便于区分 404 与网络错误
        "-w".to_string(),
        "\n%{http_code}".to_string(),
        "-H".to_string(),
        "Accept: application/json".to_string(),
        // 部分仓库（crates.io）要求带 User-Agent
        "-H".to_string(),
        "User-Agent: PackInspect/0.1 (+https://example.invalid)".to_string(),
        url.to_string(),
    ];
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let request = ExecRequest::new(curl.to_string_lossy().to_string(), &arg_refs)
        .with_timeout_ms(timeout_ms + 3_000);
    let out = executor::run_resolved(&curl, &request)?;

    // curl 在 HTTP >= 400 时仍会输出响应体，因此不把非零退出直接当失败，
    // 而是解析尾部状态码自己判断 —— 否则 404「包不存在」会被误报成网络错误。
    if out.timed_out {
        return Err(AppError::new("TIMEOUT", format!("查询超时（{seconds} 秒）")));
    }
    // 先取出失败提示，再移动 stdout（否则会 borrow-after-move）
    let failure_hint = out.failure_hint();
    let raw = out.stdout;
    let (body, status) = match raw.rfind('\n') {
        Some(idx) => (raw[..idx].to_string(), raw[idx + 1..].trim().to_string()),
        None => (raw, String::new()),
    };

    if status.is_empty() {
        // 拿不到状态码 → curl 本身失败（DNS / 代理 / TLS）
        return Err(AppError::io(failure_hint.chars().take(160).collect::<String>()));
    }
    if status == "404" {
        // 404 表示「包不存在」，属于正常业务结果
        return Err(AppError::new("NOT_FOUND", "仓库中不存在该包"));
    }
    if !status.starts_with('2') {
        return Err(AppError::io(format!("仓库返回 HTTP {status}")));
    }
    Ok(body)
}

/// 搜索可安装的包
pub fn browse(request: &BrowseRequest) -> AppResult<BrowseResult> {
    validate::search_query(&request.query)?;
    if crate::whitelist::find(&request.manager).is_none() {
        return Err(AppError::invalid(format!("不支持的包管理器: {}", request.manager)));
    }

    let Some((endpoint, kind)) = endpoint_for(&request.manager) else {
        return Ok(BrowseResult {
            packages: Vec::new(),
            attempted: false,
            failed: false,
            note: Some(format!("{} 暂未接入在线仓库查询", request.manager)),
            hint: None,
        });
    };

    let limit = request.limit.clamp(1, 50);
    let timeout = request.timeout_ms.unwrap_or(15_000).clamp(3_000, 40_000);
    let query = encode_query(&request.query);
    let hint = capability_note(&request.manager);

    // winget 走本地客户端命令（比 API 更权威）
    if kind == Kind::WingetCli {
        return Ok(match browse_winget(&request.query, limit, timeout) {
            Ok(packages) => BrowseResult {
                packages,
                attempted: true,
                failed: false,
                note: None,
                hint,
            },
            Err(e) => BrowseResult {
                packages: Vec::new(),
                attempted: true,
                failed: true,
                note: Some(e.message),
                hint,
            },
        });
    }

    let url = endpoint.replace("{q}", &query).replace("{limit}", &limit.to_string());
    let body = match http_get(&url, timeout) {
        Ok(body) => body,
        // 404 = 没有这个包，不是错误
        Err(e) if e.code == "NOT_FOUND" => {
            return Ok(BrowseResult {
                packages: Vec::new(),
                attempted: true,
                failed: false,
                note: None,
                hint,
            })
        }
        Err(e) => {
            return Ok(BrowseResult {
                packages: Vec::new(),
                attempted: true,
                failed: true,
                note: Some(e.message),
                hint,
            })
        }
    };

    let mut packages = parse_response(kind, &body, &request.manager);
    packages.truncate(limit);

    // 响应体非空却解析不出条目 → 可能是上游改了格式，如实告知而不是沉默
    let parse_issue = packages.is_empty()
        && !body.trim().is_empty()
        && !matches!(kind, Kind::PypiExact | Kind::PowerShellGallery);

    Ok(BrowseResult {
        packages,
        attempted: true,
        failed: parse_issue,
        note: if parse_issue {
            Some("仓库返回了数据但未能解析出条目，可能是上游接口格式变化".into())
        } else {
            None
        },
        hint,
    })
}

/// `winget search` 的输出是定宽表格，复用 packages.rs 里的列切分思路
fn browse_winget(query: &str, limit: usize, timeout_ms: u64) -> AppResult<Vec<RemotePackage>> {
    let Some(exe) = executor::resolve_executable(&["winget.exe", "winget"]) else {
        return Err(AppError::not_installed("winget"));
    };
    let args = vec!["search", query, "--disable-interactivity", "--accept-source-agreements"];
    let request =
        ExecRequest::new(exe.to_string_lossy().to_string(), &args).with_timeout_ms(timeout_ms);
    let out = executor::run_resolved(&exe, &request)?;
    if !out.success && out.stdout.trim().is_empty() {
        return Err(AppError::io(format!("winget search 失败: {}", out.failure_hint())));
    }
    Ok(parse_winget_search(&out.stdout, limit))
}

/// `winget search` 的输出是定宽表格。
///
/// 两个实测坑：
/// 1. **search 比 list 多一个 `Match` 列**，列位置随内容变化 —— 必须按表头实际
///    位置推导区间。曾经对列名按**字母序**排序过，结果区间颠倒、全解析出空值。
/// 2. 商店（msstore）条目返回 `XPDFF77QZ71XD0` 这类 Store ID 且版本为 `Unknown`，
///    对用户没有意义，应过滤掉 —— 否则搜 "git" 时首条就是它。
fn parse_winget_search(text: &str, limit: usize) -> Vec<RemotePackage> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let header_idx = lines.iter().position(|l| {
        let lower = l.to_ascii_lowercase();
        lower.contains("name") && lower.contains("id")
    });
    let Some(header_idx) = header_idx else { return out };

    // 列名统一下标为小写，作为 map 的键；位置按下标升序。
    // 注意 `match` 也必须作为列识别出来 —— 它本身不需要取值，
    // 但它必须是 `version` 的**右边界**，否则版本会把 "Tag: git" 一起吞进去。
    let mut columns: Vec<(String, usize)> = Vec::new();
    let mut search_from = 0usize;
    for token in lines[header_idx].split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if !matches!(lower.as_str(), "name" | "id" | "version" | "match" | "source") {
            continue;
        }
        let Some(idx) = lines[header_idx][search_from..].find(token).map(|i| i + search_from) else {
            continue;
        };
        columns.push((lower, idx));
        search_from = idx + token.len();
    }
    columns.sort_by_key(|(_, idx)| *idx);
    if !columns.iter().any(|(k, _)| k == "id") {
        return out;
    }

    for line in lines.iter().skip(header_idx + 1) {
        if out.len() >= limit {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.chars().all(|c| matches!(c, '-' | '\\' | '/' | ' ')) {
            continue;
        }
        // 每列的区间 = [本列起点, 下一列起点)
        let mut fields: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for (pos, (key, start)) in columns.iter().enumerate() {
            let end = columns.get(pos + 1).map(|(_, i)| *i).unwrap_or(usize::MAX).min(line.len());
            let value = if *start <= end { line.get(*start..end).unwrap_or("") } else { "" };
            fields.insert(key.as_str(), value.trim());
        }

        let id = fields.get("id").copied().unwrap_or("").to_string();
        if id.is_empty() || id.eq_ignore_ascii_case("id") {
            continue;
        }
        // 商店条目：版本 Unknown 且 ID 不含 "." —— 对用户没有意义
        let version = fields
            .get("version")
            .copied()
            .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("unknown"))
            .map(str::to_string);
        if version.is_none() && !id.contains('.') {
            continue;
        }
        out.push(RemotePackage {
            homepage: crate::actions::homepage_url("winget", &id),
            install_command: Some(format!("winget install {id}")),
            name: id,
            version,
            description: fields
                .get("name")
                .copied()
                .filter(|v| !v.is_empty())
                .map(str::to_string),
            downloads: None,
        });
    }
    out
}

/// 解析各家 API 的返回结构
fn parse_response(kind: Kind, body: &str, manager: &str) -> Vec<RemotePackage> {
    // PowerShell Gallery 是 Atom XML，PyPI 是单个对象，单独处理
    match kind {
        Kind::PowerShellGallery => return parse_psgallery_xml(body),
        Kind::PypiExact => return parse_pypi_exact(body),
        Kind::WingetCli => return Vec::new(),
        _ => {}
    }

    let Ok(value) = fsutil::parse_json_output(body) else {
        return Vec::new();
    };

    match kind {
        // npm: { objects: [ { package: {...}, downloads: { monthly } } ] }
        Kind::Npm => value
            .get("objects")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let pkg = item.get("package")?;
                        let name = pkg.get("name").and_then(Value::as_str)?.to_string();
                        Some(RemotePackage {
                            description: pkg
                                .get("description")
                                .and_then(Value::as_str)
                                .map(|s| s.chars().take(200).collect()),
                            version: pkg.get("version").and_then(Value::as_str).map(str::to_string),
                            homepage: crate::actions::homepage_url(manager, &name),
                            install_command: Some(format!("{manager} install -g {name}")),
                            // 真实响应里下载量在 downloads.monthly（早期实现误取了 score.detail）
                            downloads: item
                                .get("downloads")
                                .and_then(|d| d.get("monthly"))
                                .and_then(Value::as_u64),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),

        // crates.io: { crates: [ { name, max_version, description, downloads } ] }
        Kind::Crates => value
            .get("crates")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let name = item.get("name").and_then(Value::as_str)?.to_string();
                        Some(RemotePackage {
                            description: item
                                .get("description")
                                .and_then(Value::as_str)
                                .map(|s| s.chars().take(200).collect()),
                            version: item
                                .get("max_version")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            downloads: item.get("downloads").and_then(Value::as_u64),
                            homepage: crate::actions::homepage_url("cargo", &name),
                            install_command: Some(format!("cargo install {name}")),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),

        // NuGet: { data: [ { id, version, description, totalDownloads } ] }
        Kind::Nuget => value
            .get("data")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let name = item.get("id").and_then(Value::as_str)?.to_string();
                        Some(RemotePackage {
                            description: item
                                .get("description")
                                .and_then(Value::as_str)
                                .map(|s| s.chars().take(200).collect()),
                            version: item.get("version").and_then(Value::as_str).map(str::to_string),
                            downloads: item.get("totalDownloads").and_then(Value::as_u64),
                            homepage: crate::actions::homepage_url("dotnet", &name),
                            install_command: Some(format!("dotnet add package {name}")),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),

        // Packagist: { results: [ { name, description, downloads, repository } ] }
        Kind::Packagist => value
            .get("results")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let name = item.get("name").and_then(Value::as_str)?.to_string();
                        Some(RemotePackage {
                            description: item
                                .get("description")
                                .and_then(Value::as_str)
                                .map(|s| s.chars().take(200).collect()),
                            version: None,
                            downloads: item.get("downloads").and_then(Value::as_u64),
                            homepage: item
                                .get("repository")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                                .or_else(|| crate::actions::homepage_url("composer", &name)),
                            install_command: Some(format!("composer global require {name}")),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),

        // RubyGems: [ { name, version, info, downloads } ]
        Kind::Rubygems => value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let name = item.get("name").and_then(Value::as_str)?.to_string();
                        Some(RemotePackage {
                            description: item
                                .get("info")
                                .and_then(Value::as_str)
                                .map(|s| s.chars().take(200).collect()),
                            version: item.get("version").and_then(Value::as_str).map(str::to_string),
                            downloads: item.get("downloads").and_then(Value::as_u64),
                            homepage: crate::actions::homepage_url("gem", &name),
                            install_command: Some(format!("gem install {name}")),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),

        // pub.dev: { packages: [ { package } ] }（列表只有名字）
        Kind::Pub => value
            .get("packages")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let name = item.get("package").and_then(Value::as_str)?.to_string();
                        Some(RemotePackage {
                            homepage: crate::actions::homepage_url("dart", &name),
                            install_command: Some(format!("dart pub global activate {name}")),
                            name,
                            version: None,
                            description: None,
                            downloads: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),

        Kind::PowerShellGallery | Kind::PypiExact | Kind::WingetCli => Vec::new(),
    }
}

/// PyPI 精确查询：`/pypi/<name>/json` 返回单个包对象。
///
/// 取 `info.name` 作为规范名，因此用户输入 `requests` 或 `Requests` 都能拿到正确结果。
fn parse_pypi_exact(body: &str) -> Vec<RemotePackage> {
    let Ok(value) = fsutil::parse_json_output(body) else {
        return Vec::new();
    };
    let Some(info) = value.get("info") else { return Vec::new() };
    let Some(name) = info.get("name").and_then(Value::as_str) else {
        return Vec::new();
    };
    vec![RemotePackage {
        name: name.to_string(),
        version: info.get("version").and_then(Value::as_str).map(str::to_string),
        description: info
            .get("summary")
            .and_then(Value::as_str)
            .map(|s| s.chars().take(200).collect()),
        // PyPI 不返回下载量，留空而不是编一个
        downloads: None,
        homepage: crate::actions::homepage_url("pip", name),
        install_command: Some(format!("pip install {name}")),
    }]
}

/// PowerShell Gallery 返回 Atom/OData XML。
///
/// 刻意不引入 XML 解析库：只需要几个字段，字符串切分足够且无依赖。
///
/// 关键细节（实测踩过）：`<d:Version>` 是 NuGet **范围格式**，形如 `[6.2.0, )`，
/// 干净版本号在 `<d:NormalizedVersion>` 里。之前直接取 Version 会把
/// `[6.2.0, )` 原样显示给用户。
fn parse_psgallery_xml(body: &str) -> Vec<RemotePackage> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("<entry>") {
        let after = &rest[start + 7..];
        let Some(end) = after.find("</entry>") else { break };
        let entry = &after[..end];

        // 取出 `d:<tag>...</d:...>` 的文本
        let pick = |tag: &str| -> Option<String> {
            let open = format!("<d:{tag}");
            let pos = entry.find(&open)?;
            let gt = entry[pos..].find('>')? + pos + 1;
            let close = entry[gt..].find("</d:")? + gt;
            let text = entry[gt..close].trim().to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        };

        if let Some(name) = pick("Id").or_else(|| pick("Title")) {
            // 优先 NormalizedVersion；否则用 Version 并剥掉范围语法
            let version = pick("NormalizedVersion")
                .or_else(|| pick("Version"))
                .and_then(|raw| {
                    let core = raw
                        .trim_matches(['[', ']', '(', ')', ' '])
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if core.is_empty() {
                        None
                    } else {
                        Some(core)
                    }
                });
            out.push(RemotePackage {
                homepage: crate::actions::homepage_url("powershellget", &name),
                install_command: Some(format!("Install-Module {name}")),
                name,
                version,
                description: pick("Description").map(|d| d.chars().take(200).collect()),
                downloads: pick("DownloadCount").and_then(|d| d.parse::<u64>().ok()),
            });
        }
        rest = &after[end + 8..];
    }
    out
}

/// 生成安装方案：只给出命令，不执行
pub fn install_plan(manager: &str, package: &str) -> AppResult<InstallPlan> {
    validate::package_name(package)?;
    if crate::whitelist::find(manager).is_none() {
        return Err(AppError::invalid(format!("不支持的包管理器: {manager}")));
    }

    let command = crate::actions::install_command_for(manager, package)
        .ok_or_else(|| AppError::invalid(format!("{manager} 没有已知的安装命令模板")))?;

    let requires_admin = matches!(manager, "chocolatey" | "winget" | "scoop");
    Ok(InstallPlan {
        manager_id: manager.to_string(),
        package: package.to_string(),
        command,
        online: true,
        requires_admin,
        explanation: format!(
            "PackInspect 不会代替你执行安装。请复制上面的命令到终端运行 —— {manager} 的依赖解析、\
             权限确认与交互提示无法在后台可靠完成，代为执行可能污染或破坏你的环境。"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_queries_safely() {
        assert_eq!(encode_query("vue"), "vue");
        assert_eq!(encode_query("vue router"), "vue+router");
        assert_eq!(encode_query("a&b=c"), "a%26b%3Dc");
        assert_eq!(encode_query("中文"), "%E4%B8%AD%E6%96%87");
        let encoded = encode_query("x?y#z");
        assert!(!encoded.contains('?'), "? 必须转义");
        assert!(!encoded.contains('#'), "# 必须转义");
        assert_eq!(encoded, "x%3Fy%23z");
    }

    #[test]
    fn path_traversal_query_is_rejected_by_validation() {
        assert!(crate::validate::search_query("../../etc/passwd").is_err());
        assert!(crate::validate::search_query("x?y#z").is_err());
        assert!(crate::validate::search_query("a\\b").is_err());
    }

    #[test]
    fn rejects_unknown_manager_and_bad_query() {
        let bad_manager = BrowseRequest {
            manager: "not-a-manager".into(),
            query: "vue".into(),
            limit: 10,
            timeout_ms: None,
        };
        assert!(browse(&bad_manager).is_err());

        let bad_query =
            BrowseRequest { manager: "npm".into(), query: "   ".into(), limit: 10, timeout_ms: None };
        assert!(browse(&bad_query).is_err());

        let injection = BrowseRequest {
            manager: "npm".into(),
            query: "vue\" ; rm -rf /".into(),
            limit: 10,
            timeout_ms: None,
        };
        assert!(browse(&injection).is_err(), "含 shell 元字符的关键词应被拒绝");
    }

    /// 未接入在线查询的生态应返回 attempted=false，而不是假装查过
    #[test]
    fn unsupported_ecosystem_reports_not_attempted() {
        let request = BrowseRequest {
            manager: "maven".into(),
            query: "junit".into(),
            limit: 10,
            timeout_ms: None,
        };
        let result = browse(&request).unwrap();
        assert!(!result.attempted);
        assert!(result.packages.is_empty());
        assert!(result.note.is_some(), "应说明原因");
    }

    #[test]
    fn pypi_advertises_its_limitation() {
        // pip 没有关键词搜索，必须给出明确提示，否则用户会以为「搜不到」
        assert!(capability_note("pip").unwrap().contains("精确包名"));
        assert!(capability_note("npm").is_none());
        let (url, kind) = endpoint_for("pip").unwrap();
        assert_eq!(kind, Kind::PypiExact);
        assert!(url.contains("/pypi/{q}/json"), "{url}");
    }

    #[test]
    fn parses_npm_search_shape() {
        // 真实响应里下载量在 downloads.monthly，早期实现误取了 score.detail.popularity
        let body = r#"{"objects":[
            {"package":{"name":"vue","version":"3.5.0","description":"The framework"},
             "downloads":{"monthly":1234567,"weekly":300000},
             "score":{"detail":{"popularity":0.98}}}
        ]}"#;
        let parsed = parse_response(Kind::Npm, body, "npm");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "vue");
        assert_eq!(parsed[0].version.as_deref(), Some("3.5.0"));
        assert_eq!(parsed[0].downloads, Some(1234567));
        assert_eq!(parsed[0].install_command.as_deref(), Some("npm install -g vue"));
    }

    #[test]
    fn parses_crates_and_nuget_and_rubygems() {
        let crates = r#"{"crates":[{"name":"ripgrep","max_version":"15.2.0","downloads":1590751,"description":"grep"}]}"#;
        let parsed = parse_response(Kind::Crates, crates, "cargo");
        assert_eq!(parsed[0].name, "ripgrep");
        assert_eq!(parsed[0].version.as_deref(), Some("15.2.0"));
        assert_eq!(parsed[0].install_command.as_deref(), Some("cargo install ripgrep"));

        let nuget = r#"{"data":[{"id":"Newtonsoft.Json","version":"13.0.4","totalDownloads":9325394018,"description":"json"}]}"#;
        let parsed = parse_response(Kind::Nuget, nuget, "dotnet");
        assert_eq!(parsed[0].name, "Newtonsoft.Json");
        assert_eq!(parsed[0].downloads, Some(9325394018));

        let gems = r#"[{"name":"rails","version":"8.1.4","downloads":795781011,"info":"web"}]"#;
        let parsed = parse_response(Kind::Rubygems, gems, "gem");
        assert_eq!(parsed[0].name, "rails");
        assert_eq!(parsed[0].install_command.as_deref(), Some("gem install rails"));
    }

    /// PowerShell Gallery 的 Version 是 NuGet 范围格式，必须优先取 NormalizedVersion
    #[test]
    fn psgallery_prefers_normalized_version() {
        let xml = r#"<?xml version="1.0"?><feed>
        <entry>
          <title type="text">Pester</title>
          <content type="application/zip" src="https://x/package/Pester/6.2.0" />
          <m:properties>
            <d:Id>Pester</d:Id>
            <d:Version>[6.2.0, )</d:Version>
            <d:NormalizedVersion>6.2.0</d:NormalizedVersion>
            <d:Description>BDD test framework</d:Description>
            <d:DownloadCount m:type="Edm.Int32">40923786</d:DownloadCount>
          </m:properties>
        </entry>
        <entry>
          <m:properties>
            <d:Id>NoNormalized</d:Id>
            <d:Version>[1.2.3, )</d:Version>
          </m:properties>
        </entry>
        </feed>"#;
        let parsed = parse_psgallery_xml(xml);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "Pester");
        assert_eq!(
            parsed[0].version.as_deref(),
            Some("6.2.0"),
            "应取 NormalizedVersion 而不是范围值"
        );
        assert_eq!(parsed[0].downloads, Some(40923786));
        assert_eq!(parsed[0].install_command.as_deref(), Some("Install-Module Pester"));
        // 没有 NormalizedVersion 时至少要把范围括号剥掉
        assert_eq!(parsed[1].version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn parses_pypi_exact_lookup() {
        let body = r#"{"info":{"name":"requests","version":"2.32.3","summary":"Python HTTP for Humans."},"releases":{}}"#;
        let parsed = parse_pypi_exact(body);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "requests");
        assert_eq!(parsed[0].version.as_deref(), Some("2.32.3"));
        assert_eq!(parsed[0].description.as_deref(), Some("Python HTTP for Humans."));
        assert_eq!(parsed[0].install_command.as_deref(), Some("pip install requests"));
        // 异常输入不能 panic
        assert!(parse_pypi_exact("{}").is_empty());
        assert!(parse_pypi_exact("not json").is_empty());
    }

    #[test]
    fn unparsable_body_yields_empty_list() {
        assert!(parse_response(Kind::Npm, "not json", "npm").is_empty());
        assert!(parse_response(Kind::Crates, "{}", "cargo").is_empty());
        assert!(parse_psgallery_xml("no entries here").is_empty());
    }

    #[test]
    fn install_plan_never_executes_and_warns_about_admin() {
        let plan = install_plan("winget", "Git.Git").unwrap();
        assert_eq!(plan.command, "winget install Git.Git");
        assert!(plan.requires_admin);
        assert!(plan.explanation.contains("不会代替你执行"));

        assert!(install_plan("npm", "vue; rm -rf /").is_err());
        assert!(install_plan("nope", "vue").is_err());
    }

    #[test]
    fn winget_search_table_is_parsed() {
        // 真实表头：search 比 list 多一个 Match 列
        let text = "\
Name         Id             Version    Match      Source
--------------------------------------------------------------------
Git          XPDFF77QZ71XD0 Unknown               msstore
Git          Git.Git        2.55.0.5              winget
Git          Microsoft.Git  2.55.0.0.10           winget
";
        let parsed = parse_winget_search(text, 10);
        // 商店条目（版本 Unknown 且 ID 不含点）应被过滤掉
        assert_eq!(parsed.len(), 2, "应过滤掉 msstore 的 Store ID 条目：{parsed:?}");
        assert_eq!(parsed[0].name, "Git.Git");
        assert_eq!(
            parsed[0].version.as_deref(),
            Some("2.55.0.5"),
            "Version 不能错位落到 Match 列"
        );
        assert_eq!(parsed[1].name, "Microsoft.Git");
    }

    /// 回归：psgallery 的 URL 模板里若出现字面空格，curl 会直接拒绝整个 URL
    #[test]
    fn psgallery_endpoint_contains_no_raw_spaces() {
        let (url, kind) = endpoint_for("powershellget").unwrap();
        assert_eq!(kind, Kind::PowerShellGallery);
        assert!(
            !url.contains(' '),
            "URL 里不能有未编码的空格，否则 curl 报 Malformed input: {url}"
        );
        assert!(url.contains("DownloadCount%20desc"), "空格应编码为 %20");
        assert!(url.contains("{q}") && url.contains("{limit}"), "模板应含占位符");
    }
}
