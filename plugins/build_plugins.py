#!/usr/bin/env python3
"""EnvHive 插件仓库构建脚本

扫描 `plugins/src/<name>/` 下的插件源码（plugin.lua + icon.svg + lib/），
为每个插件生成 `plugins/zip/<name>.zip`（zip 根目录直接放插件文件，不打一层 <name>/），
并在**仓库根** `manifest.json` 生成清单（schema v2，downloadUrl 为相对 manifest.json
所在目录的路径 `plugins/zip/<name>.zip`），同时生成兼容版 `plugins/manifest.json`
（downloadUrl 为相对 `plugins/` 目录的 `zip/<name>.zip`，供旧版仓库根地址拉取）。

下载地址约定（宿主默认仓库地址，见 app/src-tauri/crates/envhive-core/src/config.rs）：
- Gitee : https://raw.giteeusercontent.com/envhive/envhive/raw/main/manifest.json（仓库名「官方gitee」）
- GitHub: https://raw.githubusercontent.com/envhive/envhive/main/manifest.json（仓库名「官方github」）
manifest 内 downloadUrl 可为完整下载地址，或相对 manifest.json 所在目录的路径
（如 `plugins/zip/<name>.zip`），宿主按 manifest.json 所在目录解析。

用法：
    python plugins/build_plugins.py            # 全量构建
    python plugins/build_plugins.py --verify   # 仅校验 zip 与 manifest 一致性（不重打）
"""

import argparse
import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PLUGINS_DIR = REPO_ROOT / "plugins"
SRC_DIR = PLUGINS_DIR / "src"
ZIP_DIR = PLUGINS_DIR / "zip"
# 主 manifest：仓库根（新地址，downloadUrl 相对仓库根 = manifest.json 所在目录）
MANIFEST_PATH = REPO_ROOT / "manifest.json"
# 兼容 manifest：plugins/ 目录（旧地址 {base}/plugins/manifest.json，downloadUrl 相对 plugins/）
LEGACY_MANIFEST_PATH = PLUGINS_DIR / "manifest.json"

# 必需 hook（静态检查 plugin.lua 中是否出现函数定义）
REQUIRED_HOOKS = ("available", "pre_install")
REQUIRED_TOOL_FIELDS = ("name", "display", "category", "homepage", "verify_bin")


def parse_tool_table(script: str) -> dict:
    """静态解析 Lua 脚本中的 TOOL = { ... } 表（仅取字符串/空值字段，不做完整 Lua 求值）。

    返回缺失字段为 None；字段值统一去引号去空白。
    """
    # 提取 TOOL = { ... } 花括号块（首层大括号，按深度匹配）
    m = re.search(r"\bTOOL\s*=\s*\{", script)
    if not m:
        return {}
    start = script.index("{", m.start())
    depth = 0
    in_str = None
    end = len(script)
    for i in range(start, len(script)):
        ch = script[i]
        if in_str:
            if ch == "\\":
                i += 1  # 跳过转义
            elif ch == in_str:
                in_str = None
            continue
        if ch in ('"', "'"):
            in_str = ch
        elif ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                end = i
                break
    block = script[start + 1 : end]

    out: dict = {}
    # 匹配 key = value（仅顶层：TOOL 表内条目统一为 2 空格缩进 + 行尾可选逗号/注释；
    # 嵌套表（distributions/env_vars 等）内条目缩进更深或同行，天然被排除）
    for m in re.finditer(
        r"^\s{2}([A-Za-z_][A-Za-z0-9_]*)\s*=\s*((?:\"[^\"]*\"|'[^']*'|nil|[A-Za-z0-9_.\-]+))\s*,?\s*(--.*)?$",
        block,
        re.M,
    ):
        key, val = m.group(1), m.group(2)
        if key in ("name", "display", "category", "homepage", "verify_bin", "verify_arg",
                   "bin_suffix", "root_hint", "version", "default_distribution", "default_mirror"):
            if val in ("nil", "true", "false"):
                out[key] = None if val == "nil" else val
            else:
                out[key] = val.strip("\"'")
    return out


def static_validate(name: str, src: Path) -> dict:
    """静态校验插件目录：plugin.lua 必需 + TOOL 元信息 + 必需 hook。返回解析出的元信息。"""
    errors: list[str] = []
    lua = src / "plugin.lua"
    if not lua.exists():
        errors.append(f"{name}: 缺少 plugin.lua")
        raise SystemExit("; ".join(errors))

    script = lua.read_text(encoding="utf-8")
    meta = parse_tool_table(script)

    for f in REQUIRED_TOOL_FIELDS:
        # verify_bin 允许为空字符串（解压即用型工具，如 Tomcat；与宿主 build_plugin_def 语义一致）
        if f == "verify_bin":
            if f not in meta:
                errors.append(f"{name}: TOOL 缺少字段 {f}")
            continue
        if not meta.get(f):
            errors.append(f"{name}: TOOL 缺少字段 {f}")
    for hook in REQUIRED_HOOKS:
        if not re.search(rf"\bfunction\s+{hook}\s*\(", script):
            errors.append(f"{name}: 缺少必需 hook {hook}()")
    if errors:
        raise SystemExit("; ".join(errors))
    return meta


def build_zip(name: str, src: Path, zpath: Path) -> None:
    """打 zip：zip 根目录直接放插件文件（plugin.lua / icon.svg / lib/*.lua），无顶层目录。"""
    files = sorted(p for p in src.rglob("*") if p.is_file())
    if not files:
        raise SystemExit(f"{name}: 插件目录为空")
    with zipfile.ZipFile(zpath, "w", zipfile.ZIP_DEFLATED) as zf:
        for f in files:
            # 相对 src 的路径作为 zip 内路径（含 lib/ 子目录）
            arc = f.relative_to(src).as_posix()
            zf.write(f, arc)


def sha256_hex(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def build() -> None:
    if not SRC_DIR.is_dir():
        raise SystemExit(f"插件源码目录不存在: {SRC_DIR}")
    ZIP_DIR.mkdir(parents=True, exist_ok=True)
    names = sorted(p.name for p in SRC_DIR.iterdir() if p.is_dir())
    if not names:
        raise SystemExit("插件源码目录为空")

    plugins = []
    for name in names:
        src = SRC_DIR / name
        meta = static_validate(name, src)
        version = meta.get("version") or "1.0.0"
        zpath = ZIP_DIR / f"{name}.zip"
        build_zip(name, src, zpath)
        plugins.append({
            "name": name,
            "version": version,
            "type": "lua",
            "format": "zip",
            "description": f"{meta.get('display') or name}（{meta.get('category', '')}）",
            "homepage": meta.get("homepage") or "",
            # 主 manifest 位于仓库根：相对路径即相对仓库根（= manifest.json 所在目录）
            "downloadUrl": zpath.relative_to(REPO_ROOT).as_posix(),
            "sha256": sha256_hex(zpath),
            "size": zpath.stat().st_size,
        })
        print(f"[built] {name}@{version} -> {zpath.relative_to(REPO_ROOT).as_posix()} "
              f"({plugins[-1]['size']} B)")

    manifest = {"schemaVersion": 2, "plugins": plugins}
    MANIFEST_PATH.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"[manifest] {MANIFEST_PATH.relative_to(REPO_ROOT).as_posix()} "
          f"（{len(plugins)} 个插件）")

    # 兼容版：plugins/manifest.json（旧地址 {base}/plugins/manifest.json 拉取时，
    # 相对 downloadUrl 按 manifest.json 所在目录 = plugins/ 解析）
    legacy_plugins = []
    for p in plugins:
        lp = dict(p)
        lp["downloadUrl"] = str(Path(p["downloadUrl"]).relative_to(PLUGINS_DIR.name)).replace("\\", "/")
        legacy_plugins.append(lp)
    legacy_manifest = {"schemaVersion": 2, "plugins": legacy_plugins}
    LEGACY_MANIFEST_PATH.write_text(json.dumps(legacy_manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"[manifest] {LEGACY_MANIFEST_PATH.relative_to(REPO_ROOT).as_posix()} "
          f"（兼容版，{len(legacy_plugins)} 个插件）")


def verify() -> int:
    """校验 zip 内容与 manifest 的 name/sha256/size 一致（不重打）。"""
    manifests = [(MANIFEST_PATH, REPO_ROOT), (LEGACY_MANIFEST_PATH, PLUGINS_DIR)]
    ok = True
    for manifest_path, base in manifests:
        if not manifest_path.exists():
            print(f"{manifest_path.relative_to(REPO_ROOT)} 不存在，请先运行构建", file=sys.stderr)
            ok = False
            continue
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        for p in manifest["plugins"]:
            zpath = base / p["downloadUrl"]
            if not zpath.exists():
                print(f"[missing] {manifest_path.relative_to(REPO_ROOT)}: {p['downloadUrl']}", file=sys.stderr)
                ok = False
                continue
            if sha256_hex(zpath) != p["sha256"]:
                print(f"[stale] {manifest_path.relative_to(REPO_ROOT)}: {p['downloadUrl']} sha256 与 manifest 不一致，请重新构建", file=sys.stderr)
                ok = False
        if ok:
            print(f"[verify] {manifest_path.relative_to(REPO_ROOT)} 全部 {len(manifest['plugins'])} 个 zip 校验通过")
    return 0 if ok else 1


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description="EnvHive 插件仓库构建")
    ap.add_argument("--verify", action="store_true", help="仅校验 zip 与 manifest 一致性")
    args = ap.parse_args()
    sys.exit(verify() if args.verify else (build() or 0))
