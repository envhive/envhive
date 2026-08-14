//! env 模块：作用域枚举 + 环境变量合并语义（roadmap B，对应 vfox internal/env）

pub mod registry_windows;
pub mod symlink;

use std::collections::HashMap;

use serde::Serialize;

/// 三作用域（roadmap B）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UseScope {
    Global,
    Project,
    Session,
}

impl UseScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            UseScope::Global => "global",
            UseScope::Project => "project",
            UseScope::Session => "session",
        }
    }
}

/// 环境变量集合（roadmap B：PATH 前置、Vars 后覆盖、保留用户注入）
///
/// - `vars`: `None` 表示 unset（删除该变量）
/// - `paths`: 有序、去重的 PATH 片段（高优先级在前）
#[derive(Debug, Clone, Default, Serialize)]
pub struct Envs {
    pub vars: HashMap<String, Option<String>>,
    pub paths: Vec<String>,
}

impl Envs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn var(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.vars.insert(key.into(), Some(value.into()));
        self
    }

    pub fn unset(&mut self, key: impl Into<String>) -> &mut Self {
        self.vars.insert(key.into(), None);
        self
    }

    /// 前置插入 PATH 片段（不重复；路径语义等价，见 [`path_eq`]）
    pub fn prepend_path(&mut self, path: impl Into<String>) -> &mut Self {
        let p = path.into();
        if !self.paths.iter().any(|x| path_eq(x, &p)) {
            self.paths.insert(0, p);
        }
        self
    }

    /// 追加 PATH 片段（不重复；路径语义等价，见 [`path_eq`]）
    pub fn append_path(&mut self, path: impl Into<String>) -> &mut Self {
        let p = path.into();
        if !self.paths.iter().any(|x| path_eq(x, &p)) {
            self.paths.push(p);
        }
        self
    }

    /// 合并另一个 Envs：`self` 为低优先级，`other` 为高优先级
    /// - paths：other 在前（高优先级 scope 前置插入）
    /// - vars：other 覆盖 self（后者胜出）
    pub fn extend(&mut self, other: &Envs) {
        for (k, v) in &other.vars {
            self.vars.insert(k.clone(), v.clone());
        }
        let merged: Vec<String> = other
            .paths
            .iter()
            .chain(self.paths.iter())
            .filter(|p| !p.is_empty())
            .fold(Vec::<String>::new(), |mut acc, p| {
                if !acc.iter().any(|x| path_eq(x, p)) {
                    acc.push(p.clone());
                }
                acc
            });
        self.paths = merged;
    }

    /// 转为可直接传给子进程的键值 map（PATH 合并为一个条目）
    pub fn as_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for (k, v) in &self.vars {
            if let Some(v) = v {
                map.insert(k.clone(), v.clone());
            }
        }
        let path_var = self.path_var();
        if !path_var.is_empty() {
            map.insert("PATH".to_string(), path_var.clone());
            map.insert("Path".to_string(), path_var); // Windows 大小写不敏感，双写保险
        }
        map
    }

    /// 将环境应用到子进程命令：
    /// - `vars` 中 `Some(v)` → `cmd.env(k, v)`（覆盖继承环境）
    /// - `vars` 中 `None`（unset）→ `cmd.env_remove(k)`（真正删除继承环境中的变量；
    ///   仅用 `as_map()` 无法表示删除，子进程会保留父环境旧值，导致 `Envs::unset` 失效）
    /// - 合并后的 PATH 同时写入 `PATH` 与 `Path`（Windows 大小写不敏感，双写保险）
    pub fn apply_to_command(&self, cmd: &mut std::process::Command) {
        for (k, v) in &self.vars {
            match v {
                Some(val) => {
                    cmd.env(k, val);
                }
                None => {
                    cmd.env_remove(k);
                }
            }
        }
        let path_var = self.path_var();
        if !path_var.is_empty() {
            cmd.env("PATH", &path_var);
            cmd.env("Path", &path_var);
        }
    }

    /// 生成 PATH 值（前置片段 + 现有 PATH 环境变量）
    pub fn path_var(&self) -> String {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let existing = std::env::var("PATH").unwrap_or_default();
        let mut parts: Vec<&str> = self.paths.iter().map(|s| s.as_str()).collect();
        if !existing.is_empty() {
            parts.push(&existing);
        }
        parts.join(sep)
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty() && self.paths.is_empty()
    }
}

/// 按作用域优先级合并多个 Envs（传入顺序 = 从低优先级到高优先级）
/// roadmap B：PATH 高优先级在前（前置），Vars 低优先级先并、高优先级后覆盖。
pub fn merge_by_scope_priority(envs_list: &[Envs]) -> Envs {
    let mut acc = Envs::new();
    for e in envs_list {
        acc.extend(e);
    }
    acc
}

/// 路径语义等价比较：用于 PATH 片段去重。
/// - Windows：分隔符归一（`\` ↔ `/`）+ 大小写不敏感（文件系统不区分大小写，
///   且 Rust `PathBuf` 生成的 `\` 与 Lua 插件拼接的 `/` 指向同一目录）。
/// - Unix：精确比较（分隔符唯一、大小写敏感）。
fn path_eq(a: &str, b: &str) -> bool {
    #[cfg(windows)]
    {
        let norm = |s: &str| {
            s.replace('\\', "/")
                .trim_end_matches('/')
                .to_ascii_lowercase()
        };
        norm(a) == norm(b)
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_eq_normalizes_separators_and_case() {
        #[cfg(windows)]
        {
            assert!(path_eq(r"C:\Users\a\tools\go\current\bin", "C:/Users/a/tools/go/current/bin"));
            assert!(path_eq(r"C:\Users\a\bin", r"c:\users\a\bin"));
            assert!(path_eq("C:/Users/a/bin/", r"C:\Users\a\bin"));
            assert!(!path_eq(r"C:\Users\a\bin", r"C:\Users\a\lib"));
        }
        #[cfg(not(windows))]
        {
            assert!(path_eq("/usr/local/bin", "/usr/local/bin"));
            assert!(!path_eq("/usr/local/Bin", "/usr/local/bin"));
        }
    }

    #[test]
    fn extend_dedups_path_equivalents() {
        let mut low = Envs::new();
        low.append_path(r"C:\Users\a\tools\go\current\bin");
        let mut high = Envs::new();
        high.prepend_path("C:/Users/a/tools/go/current/bin");
        low.extend(&high);
        #[cfg(windows)]
        assert_eq!(low.paths.len(), 1, "分隔符不同的等价路径应合并为一条");
        #[cfg(not(windows))]
        assert_eq!(low.paths.len(), 2);
    }
}
