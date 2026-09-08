//! 文本格式化与终端 cell 宽度工具。
//!
//! ratatui 按 Unicode 标量计数，而 CJK / 全角字符占 2 个终端 cell，
//! 直接 `format!("{:<10}")` 会错位；这里统一按 cell 宽计算。

use ratatui::style::{Color, Modifier, Style};

pub(crate) fn on_off(v: bool) -> String {
    if v { "[开]".into() } else { "[关]".into() }
}

pub(crate) fn valid_ttl(s: &str) -> bool {
    let t = s.trim();
    if t == "-1" || t == "0" {
        return true;
    }
    if let Ok(n) = t.parse::<i64>() {
        return n > 0;
    }
    if t.len() < 2 {
        return false;
    }
    let (num, unit) = t.split_at(t.len() - 1);
    num.parse::<i64>().is_ok() && matches!(unit, "h" | "m" | "s" | "d")
}

/// 字节数人性化显示
pub(crate) fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if n >= GB {
        format!("{:.1} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.0} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

/// 单字符显示宽度：CJK / 全角字符 = 2 cell，其它 = 1 cell
/// （覆盖汉字 / 平假名 / 名假名 / 韩文 / 全角符号 / 常用 CJK 扩展区）
fn cell_width(c: char) -> usize {
    let cp = c as u32;
    if cp < 0x80 {
        return 1;
    }
    if (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0x303E).contains(&cp)
        || (0x3041..=0x33FF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x4E00..=0x9FFF).contains(&cp)
        || (0xA000..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFE30..=0xFE4F).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
        || (0x20000..=0x2FFFD).contains(&cp)
        || (0x30000..=0x3FFFD).contains(&cp)
    {
        return 2;
    }
    1
}

/// 字符串按 cell 计的显示宽度
fn display_width(s: &str) -> usize {
    s.chars().map(cell_width).sum()
}

/// 按 cell 截断字符串（超出 max 时末尾追加 `…`）
pub(crate) fn truncate_cells(s: &str, max_cells: usize) -> String {
    let ell = '…';
    let ell_w = cell_width(ell);
    if display_width(s) <= max_cells {
        return s.to_string();
    }
    let limit = max_cells.saturating_sub(ell_w);
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = cell_width(c);
        if w + cw > limit {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push(ell);
    out
}

/// 右侧补空格到目标 cell 宽（不足不截断，超出原样返回）
pub(crate) fn pad_cells(s: &str, target: usize) -> String {
    let w = display_width(s);
    if w >= target {
        s.to_string()
    } else {
        let mut out = String::with_capacity(target);
        out.push_str(s);
        for _ in 0..(target - w) {
            out.push(' ');
        }
        out
    }
}

pub(crate) fn tab_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// 循环移动选中索引（越界回绕）
pub(crate) fn shift(idx: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (idx as isize + delta).rem_euclid(len as isize) as usize
}
