//! go 镜像：不直接改文件，走 `go env -w GOPROXY=...` 交由 go 工具链管理（roadmap H）。

use std::path::PathBuf;
use std::process::Command;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::registry::{RegistryPreset, RegistryState, ToolRegistryWriter};

pub struct GoWriter;

impl GoWriter {
    fn go_env(&self, key: &str) -> Result<Option<String>> {
        let mut cmd = Command::new("go");
        cmd.arg("env").arg(key);
        envhive_core::util::hide_console(&mut cmd);
        let out = cmd
            .output()
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Install, "未找到 go 命令", e))?;
        if !out.status.success() {
            return Ok(None);
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if s.is_empty() || s == "<nil>" { None } else { Some(s) })
    }
}

impl ToolRegistryWriter for GoWriter {
    fn tool_name(&self) -> &str {
        "go"
    }

    fn config_path(&self) -> Result<PathBuf> {
        // go 配置由 go env -w 管理（写入 $GOENV 文件），此处仅展示
        Ok(PathBuf::from("go env -w"))
    }

    fn read_current(&self) -> Result<RegistryState> {
        let current = self.go_env("GOPROXY")?;
        let preset_name = current
            .as_deref()
            .and_then(|u| crate::registry::all_presets().iter().find(|p| p.tool == "go" && u.contains(p.url.trim_end_matches(",direct"))).map(|p| p.name.to_string()));
        Ok(RegistryState {
            tool: "go".into(),
            current_url: current,
            config_file: None,
            preset_name,
        })
    }

    fn apply(&self, preset: &RegistryPreset) -> Result<()> {
        let value = if preset.name == "official" {
            "https://proxy.golang.org,direct".to_string()
        } else {
            preset.url.to_string()
        };
        let mut cmd = Command::new("go");
        cmd.arg("env").arg("-w").arg(format!("GOPROXY={value}"));
        envhive_core::util::hide_console(&mut cmd);
        let out = cmd
            .output()
            .map_err(|e| EnvHiveError::with_source(EnvHiveErrorKind::Install, "执行 go env -w 失败（未安装 Go？）", e))?;
        if !out.status.success() {
            return Err(EnvHiveError::new(
                EnvHiveErrorKind::Install,
                format!("go env -w 失败：{}", String::from_utf8_lossy(&out.stderr).trim()),
            ));
        }
        tracing::info!("[go] GOPROXY -> {value}");
        Ok(())
    }

    fn verify(&self, preset: &RegistryPreset) -> Result<()> {
        let cur = self.read_current()?;
        let ok = cur
            .current_url
            .as_deref()
            .map(|u| u.starts_with(preset.url.trim_end_matches(",direct")) || preset.url.contains(u))
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err(EnvHiveError::new(EnvHiveErrorKind::Registry, format!("go GOPROXY 验证失败：当前 {:?}", cur.current_url)))
        }
    }
}
