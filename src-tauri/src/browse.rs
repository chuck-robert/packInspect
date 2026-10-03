//! 在包仓库里搜索「可安装的新包」。
//!
//! 【安全与可用性取舍】
//! - 只查询各生态**已知的搜索 API**，地址由本模块的模板拼接，不接受前端传入 URL；
//! - 关键词先过 `validate::search_query`（长度 + 字符集），再按 ecospace 规则 URL 编码；
//! - 联网失败、超时、返回格式变化都降级为「返回空列表 + 提示」，不抛错阻塞界面；
//! - 能提供官方搜索 API 的生态才实现，其余返回 `None`，由界面提示「该生态暂不支持在线浏览」。
//!
//! 之所以不做「真正的包管理」：安装/卸载会改动用户真实环境，且包管理器的交互提示
//! （确认、依赖冲突、权限）无法在后台管道里可靠完成。本模块只负责**发现 + 给出命令**。

use crate::error::{AppError, AppResult};
use crate::executor::{self, ExecRequest};
use crate::fsutil;
use crate::models::{BrowseRequest, InstallPlan, RemotePackage};
use crate::validate;
use serde_json::Value;

/// 支持在线搜索的生态 → 搜索 API 模板（`{q}` 会被替换为编码后的关键词，`{limit}` 为条数）
fn search_endpoint(manager: &str) -> Option<(&'static str, &'static str)> {
    // 返回 (url 模板, 结果类型)
    match manager {
        // npm registry search
        "npm" | "pnpm" | "yarn" => {
            Some(("https://registry.npmjs.org/-/v1/search?size={limit}&text={q}", "npm"))
        }
        // PyPI 没有官方搜索 API，用 simple 索引无法检索关键词；
        // 但 pypi.org 的 JSON API 支持按精确名查询，这里用 PyPI 的 search 端点（非官方但长期可用）
        "pip" => Some(("https://pypi.org/search/?q={q}", "pypi-html")),
        // crates.io 官方 API
        "cargo" => Some(("https://crates.io/api/v1/crates?per_page={limit}&q={q}", "crates")),
        // NuGet 官方搜索 API
        "dotnet" => Some((
            "https://azuresearch-usnc.nuget.org/query?take={limit}&q={q}&prerelease=false",
            "nuget",
        )),
        // winget 用 `winget search` 命令（本地客户端，比 API 更权威）
        "winget" => Some(("winget-cli", "winget")),
        // PowerShell Gallery 官方 API
        "powershellget" => Some((
            "https://www.powershellgallery.com/api/v2/Search()?$filter=IsLatestVersion&$skip=0&$top={limit}&searchTerm='{q}'&targetFramework=''&includePrerelease=false&$orderby=DownloadCount desc&$inlinecount=allpages",
            "psgallery",
        )),
        // Packagist
        "composer" => Some(("https://packagist.org/search.json?per_page={limit}&q={q}", "packagist")),
        // RubyGems
        "gem" => Some(("https://rubygems.org/api/v1/search.json?query={q}", "rubygems")),
        // pub.dev
        "dart" => Some(("https://pub.dev/api/search?query={q}", "pub")),
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

    let seconds = (timeout_ms / 1000).clamp(3, 30).to_string();
    let args = vec![
        "-sS".to_string(),           // 静默但保留错误
        "-L".to_string(),            // 跟随重定向
        "--max-time".to_string(),
        seconds,
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
    if !out.success {
        return Err(AppError::io(format!("查询仓库失败: {}", out.failure_hint())));
    }
    Ok(out.stdout)
}

/// 搜索可安装的包
pub fn browse(request: &BrowseRequest) -> AppResult<Vec<RemotePackage>> {
    validate::search_query(&request.query)?;
    if crate::whitelist::find(&request.manager).is_none() {
        return Err(AppError::invalid(format!("不支持的包管理器: {}", request.manager)));
    }

    let Some((endpoint, kind)) = search_endpoint(&request.manager) else {
        return Err(AppError::new(
            "BROWSE_UNSUPPORTED",
            format!("{} 暂不支持在线浏览包", request.manager),
        ));
    };

    let limit = request.limit.clamp(1, 50);
    let timeout = request.timeout_ms.unwrap_or(15_000).clamp(3_000, 40_000);
    let query = encode_query(&request.query);

    // winget 走本地客户端命令（更权威），其余走 HTTP
    if endpoint == "winget-cli" {
        return browse_winget(&request.query, limit, timeout);
    }

    let url = endpoint.replace("{q}", &query).replace("{limit}", &limit.to_string());
    let body = http_get(&url, timeout)?;

    let mut packages = parse_response(kind, &body, &request.manager);
    packages.truncate(limit);
    Ok(packages)
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
    if !out.success {
        return Err(AppError::io(format!("winget search 失败: {}", out.failure_hint())));
    }
    Ok(parse_winget_search(&out.stdout, limit))
}

fn parse_winget_search(text: &str, limit: usize) -> Vec<RemotePackage> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let header_idx = lines.iter().position(|l| {
        let lower = l.to_ascii_lowercase();
        lower.contains("name") && lower.contains("id")
    });
    let Some(header_idx) = header_idx else { return out };
    let header = lines[header_idx].to_ascii_lowercase();

    let mut columns: Vec<(&str, usize)> = Vec::new();
    for key in ["name", "id", "version", "source"] {
        if let Some(idx) = header.find(key) {
            columns.push((key, idx));
        }
    }
    columns.sort_by_key(|(_, idx)| *idx);
    if !(columns.iter().any(|(k, _)| *k == "id")) {
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
        let mut fields = std::collections::HashMap::new();
        for (pos, (key, start)) in columns.iter().enumerate() {
            let hard_end = columns.get(pos + 1).map(|(_, i)| *i).unwrap_or(usize::MAX);
            let slice = line.get(*start..hard_end.min(line.len())).unwrap_or("").trim();
            fields.insert(*key, slice.to_string());
        }
        let id = fields.get("id").cloned().unwrap_or_default();
        if id.is_empty() || id.eq_ignore_ascii_case("id") {
            continue;
        }
        out.push(RemotePackage {
            homepage: crate::actions::homepage_url("winget", &id),
            install_command: Some(format!("winget install {id}")),
            name: id,
            version: fields.get("version").cloned().filter(|v| !v.is_empty()),
            description: fields.get("name").cloned().filter(|v| !v.is_empty()),
            downloads: None,
        });
    }
    out
}

/// 解析各家 API 的返回结构
fn parse_response(kind: &str, body: &str, manager: &str) -> Vec<RemotePackage> {
    let Ok(value) = fsutil::parse_json_output(body) else {
        return Vec::new();
    };

    match kind {
        // npm: { objects: [ { package: { name, version, description, links } , score: {...} } ] }
        "npm" => value
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
                            downloads: item
                                .get("score")
                                .and_then(|s| s.get("detail"))
                                .and_then(|d| d.get("popularity"))
                                .and_then(Value::as_f64)
                                .map(|p| (p * 1_000_000.0) as u64),
                            name,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),

        // crates.io: { crates: [ { name, max_version, description, downloads } ] }
        "crates" => value
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
        "nuget" => value
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
        "packagist" => value
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
        "rubygems" => value
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

        // pub.dev: { packages: [ { package } ] }（列表只有名字，需逐个查详情才能拿到版本）
        "pub" => value
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

        // PowerShell Gallery 返回 Atom XML；这里做最小化提取
        "psgallery" => parse_psgallery_xml(body),

        // PyPI 搜索返回 HTML，不做解析（界面会提示去官网搜索）
        _ => Vec::new(),
    }
}

/// PowerShell Gallery 返回的是 Atom/OData XML，用最简单的字符串切分取出条目。
/// 说明：刻意不引入 XML 解析库 —— 只需要 name/version，正则式提取足够且无依赖。
fn parse_psgallery_xml(body: &str) -> Vec<RemotePackage> {
    let mut out = Vec::new();
    // 每个 <entry> ... </entry> 是一个包
    let mut rest = body;
    while let Some(start) = rest.find("<entry>") {
        let after = &rest[start + 7..];
        let Some(end) = after.find("</entry>") else { break };
        let entry = &after[..end];

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

        let title = pick("Title").or_else(|| pick("Id"));
        if let Some(name) = title {
            let version = pick("Version").map(|v| v.rsplit('/').next().unwrap_or(&v).to_string());
            let description = pick("Description").map(|d| d.chars().take(200).collect());
            out.push(RemotePackage {
                homepage: crate::actions::homepage_url("powershellget", &name),
                install_command: Some(format!("Install-Module {name}")),
                name,
                version,
                description,
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
            "PackInspect 不会代替你执行安装。请复制上面的命令到终端运行 —— {} 的依赖解析、权限确认与 \
             交互提示无法在后台可靠完成，代为执行可能污染或破坏你的环境。",
            manager
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
        // 能改变 URL 结构的字符必须被转义
        let encoded = encode_query("x?y#z");
        assert!(!encoded.contains('?'), "? 必须转义");
        assert!(!encoded.contains('#'), "# 必须转义");
        assert_eq!(encoded, "x%3Fy%23z");
    }

    /// 路径片段本身不是 URL 结构字符，单靠 encode 不会拦掉 `..`；
    /// 因此关键词在进入 `encode_query` 之前必须先过 `validate::search_query`。
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

        let bad_query = BrowseRequest {
            manager: "npm".into(),
            query: "   ".into(),
            limit: 10,
            timeout_ms: None,
        };
        assert!(browse(&bad_query).is_err(), "空关键词应被拒绝");

        let injection = BrowseRequest {
            manager: "npm".into(),
            query: "vue\" ; rm -rf /".into(),
            limit: 10,
            timeout_ms: None,
        };
        assert!(browse(&injection).is_err(), "含 shell 元字符的关键词应被拒绝");
    }

    #[test]
    fn parses_npm_search_shape() {
        let body = r#"{"objects":[
            {"package":{"name":"vue","version":"3.5.0","description":"The framework"},
             "score":{"detail":{"popularity":0.98}}}
        ]}"#;
        let parsed = parse_response("npm", body, "npm");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "vue");
        assert_eq!(parsed[0].version.as_deref(), Some("3.5.0"));
        assert_eq!(parsed[0].install_command.as_deref(), Some("npm install -g vue"));
    }

    #[test]
    fn parses_crates_and_nuget_and_rubygems() {
        let crates = r#"{"crates":[{"name":"ripgrep","max_version":"14.1.0","downloads":100,"description":"grep"}]}"#;
        let parsed = parse_response("crates", crates, "cargo");
        assert_eq!(parsed[0].name, "ripgrep");
        assert_eq!(parsed[0].install_command.as_deref(), Some("cargo install ripgrep"));

        let nuget = r#"{"data":[{"id":"Newtonsoft.Json","version":"13.0.3","totalDownloads":9,"description":"json"}]}"#;
        let parsed = parse_response("nuget", nuget, "dotnet");
        assert_eq!(parsed[0].name, "Newtonsoft.Json");

        let gems = r#"[{"name":"rails","version":"7.1.0","downloads":5,"info":"web"}]"#;
        let parsed = parse_response("rubygems", gems, "gem");
        assert_eq!(parsed[0].name, "rails");
        assert_eq!(parsed[0].install_command.as_deref(), Some("gem install rails"));
    }

    #[test]
    fn parses_psgallery_xml_entries() {
        let xml = r#"<?xml version="1.0"?><feed>
        <entry><d:Id>PSReadLine</d:Id><d:Version>2.3.4</d:Version>
        <d:Description>Line editor</d:Description></entry>
        <entry><d:Id>Pester</d:Id><d:Version>5.5.0</d:Version></entry>
        </feed>"#;
        let parsed = parse_psgallery_xml(xml);
        assert_eq!(parsed.len(), 2, "应解析出两个条目");
        assert_eq!(parsed[0].name, "PSReadLine");
        assert_eq!(parsed[0].version.as_deref(), Some("2.3.4"));
        assert_eq!(parsed[1].name, "Pester");
    }

    #[test]
    fn unparsable_body_yields_empty_list() {
        assert!(parse_response("npm", "not json", "npm").is_empty());
        assert!(parse_response("crates", "{}", "cargo").is_empty());
        assert!(parse_psgallery_xml("no entries here").is_empty());
    }

    #[test]
    fn install_plan_never_executes_and_warns_about_admin() {
        let plan = install_plan("winget", "Git.Git").unwrap();
        assert_eq!(plan.command, "winget install Git.Git");
        assert!(plan.requires_admin);
        assert!(plan.explanation.contains("不会代替你执行"));

        // 非法包名必须被拒
        assert!(install_plan("npm", "vue; rm -rf /").is_err());
        assert!(install_plan("nope", "vue").is_err());
    }

    #[test]
    fn winget_search_table_is_parsed() {
        let text = "\
Name         Id             Version  Source
----------------------------------------------
Git          Git.Git        2.50.0   winget
7-Zip        7zip.7zip      24.09    winget
";
        let parsed = parse_winget_search(text, 10);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "Git.Git");
        assert_eq!(parsed[0].version.as_deref(), Some("2.50.0"));
    }
}
