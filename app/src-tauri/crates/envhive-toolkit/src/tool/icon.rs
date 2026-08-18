//! 工具 图标解析：插件目录 `icon.svg` / `icon.png` / `icon.jpg` 文件（优先）→ 插件声明的 base64 → None
//!
//! 优先级（与 docs/tool-icon-feasibility.md 一致）：
//!   1. 插件目录内图标文件：icon.svg > icon.png > icon.jpg > icon.jpeg（≤ 512KB，读失败/超限跳过）
//!   2. 插件声明 base64（Lua `TOOL.icon_base64` / TOML `icon`；已带 data: 前缀则透传）
//!   3. 均无 → None（前端回退 DOT_COLORS 彩色圆点）
//!
//! 统一输出 `data:<mime>;base64,<...>` data URI，前端 `<img :src>` 直接渲染，
//! 无需 Tauri asset 协议 / convertFileSrc。

use std::path::Path;

/// 单文件 base64 编码（手写实现，避免为一个小函数引入依赖）
fn b64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

/// 解析 工具 图标为 data URI。
///
/// - `dir`：插件目录（真实名目录或 `<name>.disabled` 禁用目录均可，由调用方决定）
/// - `declared`：插件声明的 base64 原文（Lua `TOOL.icon_base64` / TOML `icon`；可为 data URI）
pub fn resolve_icon(dir: &Path, declared: Option<&str>) -> Option<String> {
    const MAX_ICON_BYTES: usize = 512 * 1024;
    const CANDIDATES: &[(&str, &str)] = &[
        ("icon.svg", "image/svg+xml"),
        ("icon.png", "image/png"),
        ("icon.jpg", "image/jpeg"),
        ("icon.jpeg", "image/jpeg"),
    ];
    for (file, mime) in CANDIDATES {
        let p = dir.join(file);
        let Ok(bytes) = std::fs::read(&p) else { continue };
        if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
            continue;
        }
        tracing::debug!("工具 图标: 读取 {} ({} B)", p.display(), bytes.len());
        return Some(format!("data:{mime};base64,{}", b64_encode(&bytes)));
    }
    if let Some(d) = declared {
        let d = d.trim();
        if d.is_empty() {
            return None;
        }
        if d.starts_with("data:") {
            return Some(d.to_string());
        }
        return Some(format!("data:image/png;base64,{d}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_b64_encode_known_vector() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_resolve_priority_and_fallback() {
        let root = std::env::temp_dir().join(format!("envhive-icon-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();

        // 仅声明 base64 → 返回 data URI（裸 base64 默认按 png 处理）
        let icon = resolve_icon(&root, Some("Zm9v"));
        assert_eq!(icon.as_deref(), Some("data:image/png;base64,Zm9v"));
        // 已带 data: 前缀 → 透传
        let icon2 = resolve_icon(&root, Some("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4="));
        assert_eq!(icon2.as_deref(), Some("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4="));
        // 无声明也无文件 → None
        assert_eq!(resolve_icon(&root, None), None);

        // 写入 icon.png → 文件优先于声明
        std::fs::write(root.join("icon.png"), b"PNGDATA").unwrap();
        let icon3 = resolve_icon(&root, Some("Zm9v"));
        assert!(icon3.unwrap().starts_with("data:image/png;base64,"));

        // 写入 icon.svg → svg 优先于 png
        std::fs::write(root.join("icon.svg"), b"<svg></svg>").unwrap();
        let icon4 = resolve_icon(&root, None).unwrap();
        assert!(icon4.starts_with("data:image/svg+xml;base64,"));

        // 超限文件被跳过 → 回退到声明
        std::fs::write(root.join("icon.png"), vec![0u8; 512 * 1024 + 1]).unwrap();
        std::fs::remove_file(root.join("icon.svg")).unwrap();
        let icon5 = resolve_icon(&root, Some("YmFy"));
        assert_eq!(icon5.as_deref(), Some("data:image/png;base64,YmFy"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
