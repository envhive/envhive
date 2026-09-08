//! Shell 脚本生成（envhive-toolkit）
//!
//! 供 `envhive-cli load` 输出对应 shell 语法的环境注入脚本。
//! 子进程无法修改父 shell 环境变量（操作系统限制），因此 CLI 只负责生成
//! 正确语法并做转义，由父 shell 自己执行（`eval "$(envhive-cli load)"` 等）。
//!
//! 支持：bash / zsh / fish / PowerShell / cmd
//! PATH 语义：envhive 条目前置，保留现有 $PATH 后缀；unset 语义：值 None 输出清除语句。

use envhive_core::env::Envs;

/// 目标 shell
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Cmd,
}

impl ShellKind {
    /// 从字符串解析（--shell 参数）；未知返回 None
    pub fn parse(s: &str) -> Option<ShellKind> {
        match s.to_ascii_lowercase().as_str() {
            "bash" => Some(ShellKind::Bash),
            "zsh" => Some(ShellKind::Zsh),
            "fish" => Some(ShellKind::Fish),
            "powershell" | "pwsh" | "ps1" => Some(ShellKind::PowerShell),
            "cmd" | "batch" | "bat" => Some(ShellKind::Cmd),
            _ => None,
        }
    }

    /// 自动检测：`$SHELL` 环境变量（Unix）→ 父进程名（Windows）
    pub fn detect() -> ShellKind {
        #[cfg(windows)]
        {
            // 父进程名判断：powershell.exe / pwsh.exe / cmd.exe
            if let Ok(parent) = std::env::var("__ENVHIVE_PARENT") {
                if let Some(k) = ShellKind::parse(&parent) {
                    return k;
                }
            }
            // Windows 常见场景：从 SHELL 或用户默认终端推断
            if let Ok(shell) = std::env::var("SHELL") {
                if let Some(k) = ShellKind::parse(&shell) {
                    return k;
                }
            }
            ShellKind::PowerShell
        }
        #[cfg(not(windows))]
        {
            let shell = std::env::var("SHELL").unwrap_or_default();
            if shell.contains("zsh") {
                ShellKind::Zsh
            } else if shell.contains("fish") {
                ShellKind::Fish
            } else {
                ShellKind::Bash
            }
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ShellKind::Bash => "bash",
            ShellKind::Zsh => "zsh",
            ShellKind::Fish => "fish",
            ShellKind::PowerShell => "powershell",
            ShellKind::Cmd => "cmd",
        }
    }
}

/// 生成环境注入脚本
///
/// - `envs`：合并后的环境（paths 前置 + vars）
/// - 返回脚本字符串（不含末尾换行的约定由调用方决定；此处每条语句一行）
pub fn render(envs: &Envs, shell: ShellKind) -> String {
    let mut lines: Vec<String> = Vec::new();
    if !envs.paths.is_empty() {
        lines.push(render_path(&envs.paths, shell));
    }
    // vars 排序输出（确定性，便于 diff / 测试）
    let mut vars: Vec<(&String, &Option<String>)> = envs.vars.iter().collect();
    vars.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in vars {
        match v {
            Some(val) => lines.push(render_set(k, val, shell)),
            None => lines.push(render_unset(k, shell)),
        }
    }
    lines.join("\n")
}

/// PATH 注入语句
fn render_path(paths: &[String], shell: ShellKind) -> String {
    let joined = paths.join(path_sep(shell));
    match shell {
        ShellKind::Bash | ShellKind::Zsh => {
            // `${{PATH:+:$PATH}}`：PATH 非空时追加 `:<原值>`，为空则只输出新前缀
            // （避免 PATH 为空时残留前导冒号；同时保留终端现有 PATH 后缀）
            format!("export PATH=\"{}${{PATH:+:$PATH}}\"", joined)
        }
        ShellKind::Fish => {
            // fish: set -gx PATH <new> $PATH
            let parts: Vec<String> = paths
                .iter()
                .map(|p| shell_quote_fish(p))
                .chain(std::iter::once("$PATH".to_string()))
                .collect();
            format!("set -gx PATH {}", parts.join(" "))
        }
        ShellKind::PowerShell => {
            format!("$env:PATH = \"{};\" + $env:PATH", joined)
        }
        ShellKind::Cmd => {
            format!("set \"PATH={};%PATH%\"", joined)
        }
    }
}

fn path_sep(shell: ShellKind) -> &'static str {
    match shell {
        ShellKind::Cmd | ShellKind::PowerShell => ";",
        _ => ":",
    }
}

/// 设置环境变量语句
fn render_set(key: &str, value: &str, shell: ShellKind) -> String {
    match shell {
        ShellKind::Bash | ShellKind::Zsh => format!("export {}=\"{}\"", key, escape_double_quote(value)),
        ShellKind::Fish => format!("set -gx {} {}", key, shell_quote_fish(value)),
        ShellKind::PowerShell => format!("$env:{} = \"{}\"", key, escape_double_quote(value)),
        ShellKind::Cmd => format!("set \"{}={}\"", key, escape_cmd(value)),
    }
}

/// 清除环境变量语句（unset 语义）
fn render_unset(key: &str, shell: ShellKind) -> String {
    match shell {
        ShellKind::Bash | ShellKind::Zsh => format!("unset {}", key),
        ShellKind::Fish => format!("set -e {}", key),
        ShellKind::PowerShell => format!("Remove-Item Env:{} -ErrorAction SilentlyContinue", key),
        ShellKind::Cmd => format!("set \"{}=\"", key),
    }
}

// ---------------------------------------------------------------------------
// 转义
// ---------------------------------------------------------------------------

/// 双引号内转义：`"` → `\"`（bash/zsh/powershell 共用）
fn escape_double_quote(s: &str) -> String {
    s.replace('"', "\\\"")
}

/// cmd `set "K=V"` 内：值中的 `"` 无法转义，替换为 `'`（防注入；cmd 无标准转义）
fn escape_cmd(s: &str) -> String {
    s.replace('"', "'")
}

/// fish 单引号包裹：内部 `'` → `\'`（fish 单引号内 `\` 为字面量，仅转义单引号）
fn shell_quote_fish(s: &str) -> String {
    format!("'{}'", s.replace('\'', "\\'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_envs() -> Envs {
        let mut e = Envs::new();
        e.prepend_path("C:/tools/nodejs/current/bin");
        e.prepend_path("C:/tools/go/current/bin");
        e.var("JAVA_HOME", "C:/tools/java/current");
        e.var("NODE_OPTIONS", "None"); // 不会被 unset
        e
    }

    #[test]
    fn bash_render() {
        let e = sample_envs();
        let s = render(&e, ShellKind::Bash);
        assert!(
            s.contains("export PATH=\"C:/tools/go/current/bin:C:/tools/nodejs/current/bin${PATH:+:$PATH}\""),
            "bash PATH 应保留现有 PATH：{s}"
        );
        assert!(s.contains("export JAVA_HOME=\"C:/tools/java/current\""));
    }

    #[test]
    fn bash_empty_path_no_leading_colon() {
        let e = Envs::new();
        let s = render(&e, ShellKind::Bash);
        assert!(!s.contains("PATH"), "空 env 不应输出 PATH 语句: {s}");
    }

    #[test]
    fn powershell_render() {
        let e = sample_envs();
        let s = render(&e, ShellKind::PowerShell);
        assert!(s.contains("$env:PATH = \"C:/tools/go/current/bin;C:/tools/nodejs/current/bin;\" + $env:PATH"));
        assert!(s.contains("$env:JAVA_HOME = \"C:/tools/java/current\""));
    }

    #[test]
    fn cmd_render() {
        let e = sample_envs();
        let s = render(&e, ShellKind::Cmd);
        assert!(s.contains("set \"PATH=C:/tools/go/current/bin;C:/tools/nodejs/current/bin;%PATH%\""));
        assert!(s.contains("set \"JAVA_HOME=C:/tools/java/current\""));
    }

    #[test]
    fn fish_render() {
        let e = sample_envs();
        let s = render(&e, ShellKind::Fish);
        assert!(s.contains("set -gx PATH 'C:/tools/go/current/bin' 'C:/tools/nodejs/current/bin' $PATH"));
    }

    #[test]
    fn unset_semantics() {
        let mut e = Envs::new();
        e.unset("NODE_OPTIONS");
        assert!(render(&e, ShellKind::Bash).contains("unset NODE_OPTIONS"));
        assert!(render(&e, ShellKind::PowerShell).contains("Remove-Item Env:NODE_OPTIONS"));
        assert!(render(&e, ShellKind::Cmd).contains("set \"NODE_OPTIONS=\""));
    }

    #[test]
    fn escape_quote_in_value() {
        let mut e = Envs::new();
        e.var("WEIRD", "a\"b");
        let s = render(&e, ShellKind::Bash);
        assert!(s.contains("export WEIRD=\"a\\\"b\""));
    }

    #[test]
    fn shell_parse_and_detect() {
        assert_eq!(ShellKind::parse("bash"), Some(ShellKind::Bash));
        assert_eq!(ShellKind::parse("PowerShell"), Some(ShellKind::PowerShell));
        assert_eq!(ShellKind::parse("cmd"), Some(ShellKind::Cmd));
        assert_eq!(ShellKind::parse("fish"), Some(ShellKind::Fish));
        assert_eq!(ShellKind::parse("unknown"), None);
    }
}
