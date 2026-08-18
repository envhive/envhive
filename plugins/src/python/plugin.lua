-- 蜂巢内置默认插件：Python（独立可移植构建）
-- 来源：astral-sh/python-build-standalone（与 uv / ruff 同源的绿色构建，含 pip，解压即用）
-- 布局：Linux/macOS 原生 bin/python；Windows 根目录 python.exe —— post_install 把 Windows
--   根目录整体移入 bin/（python.exe 与 python3xx.dll 保持同目录，自洽），统一为 bin/ 布局，
--   使 verify_bin / bin_suffix / env_keys 三平台一致。
TOOL = {
  name = "python",
  display = "Python",
  category = "language",
  homepage = "https://www.python.org",
  version = "1.0.0",
  verify_bin = "bin/python",
  verify_arg = "--version",
  bin_suffix = "/bin",

  -- 下载加速镜像候选：GitHub Release 资产（含 SHASUMS256.txt）走代理加速；
  -- 选「官方源」即直连 github.com。版本 API（api.github.com）不受镜像规则影响。
  mirrors = {
    {
      name = "ghproxy",
      from = "https://github.com/astral-sh/python-build-standalone/",
      to = "https://ghproxy.net/https://github.com/astral-sh/python-build-standalone/",
    },
    {
      name = "南京大学开源镜像站",
      from = "https://github.com/astral-sh/python-build-standalone/releases/download/",
      to = "https://mirror.nju.edu.cn/github-release/astral-sh/python-build-standalone/",
    },
  },
}

NETWORK_ALLOW = { "api.github.com", "github.com" }

local REPO = "astral-sh/python-build-standalone"

-- 平台 triple（PBS 资产命名约定）
local function triple(os, arch)
  if os == "win" then
    return (arch == "arm64") and "aarch64-pc-windows-msvc" or "x86_64-pc-windows-msvc"
  elseif os == "darwin" then
    return (arch == "arm64") and "aarch64-apple-darwin" or "x86_64-apple-darwin"
  end
  return (arch == "arm64") and "aarch64-unknown-linux-gnu" or "x86_64-unknown-linux-gnu"
end

-- 版本升序比较（3.8.20 < 3.13.1）
local function semver_lt(a, b)
  local pa, pb = {}, {}
  for x in a:gmatch("%d+") do table.insert(pa, tonumber(x)) end
  for x in b:gmatch("%d+") do table.insert(pb, tonumber(x)) end
  for i = 1, math.max(#pa, #pb) do
    local x, y = pa[i] or 0, pb[i] or 0
    if x ~= y then return x < y end
  end
  return false
end

-- 版本列表：latest release 资产中解析 cpython 版本（去重、升序）
-- 仅列 install_only 变体（不含编译器的精简构建，体积更小、解压即用）
function available()
  local rel = json.decode(http.get("https://api.github.com/repos/" .. REPO .. "/releases/latest"))
  local seen, out = {}, {}
  for _, a in ipairs(rel.assets or {}) do
    local n = a.name or ""
    local ver = n:match("^cpython%-([%d%.]+)%+")
    if ver and n:find("install_only", 1, true) and not seen[ver] then
      seen[ver] = true
      table.insert(out, { version = ver, labels = { "stable" } })
    end
  end
  table.sort(out, function(x, y) return semver_lt(x.version, y.version) end)
  return out
end

-- 下载：latest release 资产中定位当前平台 install_only 包（后缀自适应，不依赖 ctx.ext 猜测）
function pre_install(ctx)
  local rel = json.decode(http.get("https://api.github.com/repos/" .. REPO .. "/releases/latest"))
  local tag = rel.tag_name or ""
  local prefix = "cpython-" .. ctx.version .. "+" .. tag .. "-" .. triple(ctx.os, ctx.arch) .. "-install_only"
  local target
  for _, a in ipairs(rel.assets or {}) do
    local n = a.name or ""
    if n:sub(1, #prefix) == prefix then
      target = a
      break
    end
  end
  if not target then
    return nil
  end
  -- checksum：SHASUMS256.txt 按文件名匹配（该文件同样走镜像规则）
  local checksum
  local ok, body = pcall(http.get,
    "https://github.com/" .. REPO .. "/releases/download/" .. tag .. "/SHASUMS256.txt")
  if ok then
    for line in body:gmatch("[^\r\n]+") do
      local hash, fname = line:match("^(%x+)%s+%*?([%w%.%-_+]+)$")
      if fname == target.name then
        checksum = "sha256:" .. hash
        break
      end
    end
  end
  return {
    url = target.browser_download_url,
    file_name = target.name,
    checksum = checksum,
    root_hint = "python",
  }
end

-- Windows：PBS 包可执行文件在根目录 —— 布局归一化由 Rust 层 `normalize_windows_pbs_layout` 处理，
-- 不依赖 PowerShell / cmd / os.execute，无 shell 转义风险。
function post_install(ctx)
  -- Windows 布局已在 install.rs 中由 Rust 原生 fs::rename 完成，此处无需操作。
  -- 非 Windows 平台无特殊处理需求（PBS Unix 构建原生就是 bin/ 布局）。
end

-- 环境注入：bin 目录（三平台统一）；PBS 自包含，无需 PYTHONHOME。
-- Windows 上 pip/setuptools 装在 bin/Scripts/（post_install 已把根目录整体移入 bin/），需追加注入
function env_keys(ctx)
  local paths = { ctx.root .. "/bin" }
  if ctx.os == "win" then
    table.insert(paths, ctx.root .. "/bin/Scripts")
  end
  return { paths = paths }
end
