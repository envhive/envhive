-- 蜂巢内置默认插件：Lua（dyne/luabinaries 官方同源预编译二进制）
--
-- 来源：GitHub Releases（dyne/luabinaries，覆盖 Linux x64/arm64 · macOS x64/arm64 ·
--   Windows x64；Linux 为 musl 静态构建，macOS/Windows 为原生构建）。
-- 布局：**单文件可执行二进制**（非归档）——宿主按「单文件 工具」安装：下载文件直接
--   放入安装目录并统一命名为 <tool>[.exe]（Windows 追加 .exe），因此 verify_bin /
--   env_keys 三平台一致（Windows 安装验证自动探测 .exe）。
--   Windows 例外：exe 为 MinGW 动态构建，运行时依赖同目录 luaVV.dll → pre_install
--   通过 extra_files 附带 dll，由宿主同链路下载/校验并放入安装目录（协议新增）。
-- 版本：固定清单 5.1.5 / 5.3.6 / 5.4.8 / 5.5.0 —— Lua 版本演进极慢，且该仓库每次
--   release 将全部版本二进制一并重发（资产名不含版本号），无法由 API 推导补丁号；
--   Lua 发布新版本后在此追加即可。
-- 校验：SHA256SUMS-<platform>.txt（与资产同 release；条目路径含 build/<os>/ 前缀，
--   按 basename 匹配）。

TOOL = {
  name = "lua",
  display = "Lua",
  category = "language",
  homepage = "https://www.lua.org",
  version = "1.0.0",
  verify_bin = "lua",   -- 单文件统一名：<tool>[.exe]；Windows 自动探测 lua.exe
  verify_arg = "-v",
  bin_suffix = "",      -- 可执行文件在安装根目录，PATH 直接注入根目录

  -- 下载加速镜像候选：GitHub Release 资产（含 SHA256SUMS）走代理加速；
  -- 选「官方源」即直连 github.com。版本 API（api.github.com）不受镜像规则影响。
  mirrors = {
    {
      name = "ghproxy",
      from = "https://github.com/dyne/luabinaries/",
      to = "https://ghproxy.net/https://github.com/dyne/luabinaries/",
    },
    {
      name = "南京大学开源镜像站",
      from = "https://github.com/dyne/luabinaries/releases/download/",
      to = "https://mirror.nju.edu.cn/github-release/dyne/luabinaries/",
    },
  },
}

NETWORK_ALLOW = { "api.github.com", "github.com" }

local REPO = "dyne/luabinaries"

-- 版本清单（降序）：Lua 官方当前在维护 5.4（主流）与 5.5（新）；5.1 / 5.3 仍被大量
-- 嵌入式/游戏项目使用，一并提供。新增 Lua 版本时在此追加即可（资产短名由版本号推导）。
local VERSIONS = {
  { version = "5.5.0", labels = { "stable" } },
  { version = "5.4.8", labels = { "stable" } },
  { version = "5.3.6" },
  { version = "5.1.5" },
}

function available()
  return VERSIONS
end

-- 版本号 → 资产短名：5.4.8 → "54"；5.1.5 → "51"；5.5.0 → "55"
local function short(ver)
  local a, b = ver:match("^(%d+)%.(%d+)")
  return (a or "") .. (b or "")
end

-- 平台资产名（dyne/luabinaries 命名约定；Windows 同时分发 luaVV.dll，exe 静态自包含不依赖）
local function asset_name(os, arch, ver)
  local s = short(ver)
  if os == "win" then
    return "lua" .. s .. ".exe"
  elseif os == "linux" then
    return (arch == "arm64") and ("lua" .. s .. "-linux-arm64") or ("lua" .. s)
  elseif os == "darwin" then
    return (arch == "arm64") and ("lua" .. s .. "-macos-arm64") or ("lua" .. s .. "-macos-x64")
  end
  return nil -- 其他平台不支持
end

-- 最新 release tag（提交哈希；每次 release 全量重发全部版本资产）
local function latest_tag()
  local rel = json.decode(http.get("https://api.github.com/repos/" .. REPO .. "/releases/latest"))
  return rel.tag_name or ""
end

-- 校验文件（按平台分发）
local function sums_file(os, arch)
  if os == "win" then
    return "SHA256SUMS-windows-x64.txt"
  elseif os == "linux" then
    return (arch == "arm64") and "SHA256SUMS-linux-arm64.txt" or "SHA256SUMS-linux-x64.txt"
  elseif os == "darwin" then
    return (arch == "arm64") and "SHA256SUMS-macos-arm64.txt" or "SHA256SUMS-macos-x64.txt"
  end
  return nil
end

-- 从 SHA256SUMS 中取某资产的 sha256（条目形如 "hash  build/win64/lua54.exe"，
-- 带目录前缀，按 basename 匹配；该文件与下载同源，同样走镜像规则）
local function sha_for(sums_url, asset)
  local ok, body = pcall(http.get, sums_url)
  if not ok then
    return nil
  end
  for line in body:gmatch("[^\r\n]+") do
    local hash, fname = line:match("^(%x+)%s+%*?([^%s]+)$")
    if hash and fname then
      local base = fname:match("([^/\\]+)$")
      if base == asset then
        return "sha256:" .. hash
      end
    end
  end
  return nil
end

function pre_install(ctx)
  local name = asset_name(ctx.os, ctx.arch, ctx.version)
  if not name then
    return nil
  end
  local tag = latest_tag()
  if tag == "" then
    return nil
  end
  local base_url = "https://github.com/" .. REPO .. "/releases/download/" .. tag
  local url = base_url .. "/" .. name
  local sums = sums_file(ctx.os, ctx.arch)

  -- checksum：SHA256SUMS-<platform>.txt 按资产 basename 匹配
  local checksum
  if sums then
    checksum = sha_for(base_url .. "/" .. sums, name)
  end

  -- Windows：exe 为 MinGW 动态构建，运行时依赖同目录 luaVV.dll → 附加文件一并下载
  -- （宿主负责下载与校验，与主文件同链路：镜像 + checksum + 进度事件）
  local extra_files = {}
  if ctx.os == "win" then
    local dll = "lua" .. short(ctx.version) .. ".dll"
    local dll_checksum
    if sums then
      dll_checksum = sha_for(base_url .. "/" .. sums, dll)
    end
    table.insert(extra_files, {
      url = base_url .. "/" .. dll,
      file_name = dll,
      checksum = dll_checksum,
    })
  end

  return { url = url, file_name = name, checksum = checksum, extra_files = extra_files }
end

-- 环境注入：可执行文件在安装根目录（单文件布局，无 bin/ 子目录）
function env_keys(ctx)
  return { paths = { ctx.root } }
end
