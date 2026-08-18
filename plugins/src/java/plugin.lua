-- 蜂巢内置默认插件：Java（多发行商协议，docs/lua-plugin-design.md）
-- 发行商维度：8 个（open/tem/bisheng/corretto/graal/zulu/albba/librca）。
-- JavaFX 不作为独立维度 —— 是否含 FX 直接体现在版本条目（SDKMAN 同款格式）：
--   "26.0.2-open"（不含 FX）   "26.0.2.fx-open"（含 JavaFX）
-- 版本标识符为不透明字符串，宿主全程透传；本插件用内置 versions.parse() 拆回
--   { version, distribution, javafx } 决定各发行商 API 参数。

TOOL = {
  name = "java",
  display = "Java",
  category = "language",
  homepage = "https://adoptium.net",
  version = "1.0.0",
  verify_bin = "bin/java",
  verify_arg = "-version",
  bin_suffix = "/bin",
  env_vars = { JAVA_HOME = "{root}" },

  distributions = {
    { key = "open",     display = "OpenJDK (Eclipse)", default = true },
    { key = "tem",      display = "Eclipse Temurin" },
    { key = "bisheng",  display = "Bisheng (Huawei)" },
    { key = "corretto", display = "Corretto (Amazon)" },
    { key = "graal",    display = "GraalVM (Oracle)" },
    { key = "zulu",     display = "Azul Zulu" },
    { key = "albba",    display = "Dragonwell (Alibaba)" },
    { key = "librca",   display = "Liberica (BellSoft)" },
  },
  default_distribution = "open",
}

NETWORK_ALLOW = {
  "api.adoptium.net", "api.azul.com", "api.bell-sw.com", "api.github.com", "github.com",
}

-- ---------------------------------------------------------------------------
-- 各发行商信息：feature 大版本清单（顺序即展示顺序）与是否提供 JavaFX 构建
-- ---------------------------------------------------------------------------
local DIST_INFO = {
  open     = { features = { 8, 11, 17, 21, 25, 26 }, fx = true },
  tem      = { features = { 8, 11, 17, 21, 25, 26 }, fx = true },
  zulu     = { features = { 8, 11, 17, 21, 25, 26 }, fx = true },
  librca   = { features = { 8, 11, 17, 21, 25, 26 }, fx = true },
  corretto = { features = { 8, 11, 17, 21 },         fx = false },
  bisheng  = { features = { 8, 11, 17, 21 },         fx = false },
  albba    = { features = { 8, 11, 17, 21 },         fx = false },
  graal    = { features = { 17, 21, 23 },            fx = false }, -- CE 冻结于 23.0.2
}

-- GitHub Releases 仓库：dist → (feature) -> "owner/repo"
local GITHUB_REPO = {
  corretto = function(f) return "corretto/corretto-" .. f end,
  bisheng  = function(f) return "huawei/bishengjdk-" .. f end,
  albba    = function(f) return "dragonwell-project/dragonwell" .. f end,
  graal    = function(f) return "graalvm/graalvm-ce-builds" end,
}

-- ---------------------------------------------------------------------------
-- available(ctx)：当前发行商的版本条目（feature 大版本 + [.fx] 变体）
-- 顺序 = 展示顺序（宿主不重排）；FX 变体紧邻对应非 FX 版本。
-- ---------------------------------------------------------------------------
function available(ctx)
  local dist = ctx.distribution or TOOL.default_distribution or "open"
  local info = DIST_INFO[dist]
  if not info then
    return { { version = "21-" .. dist, labels = { "lts", "stable" } } }
  end
  local out = {}
  for _, f in ipairs(info.features) do
    local labels = { "stable" }
    if f == 8 or f == 11 or f == 17 or f == 21 or f == 25 then
      labels = { "lts", "stable" }
    end
    table.insert(out, { version = tostring(f) .. "-" .. dist, labels = labels })
    if info.fx then
      table.insert(out, { version = tostring(f) .. ".fx-" .. dist, labels = { "lts", "stable", "jfx" } })
    end
  end
  return out
end

-- ---------------------------------------------------------------------------
-- pre_install(ctx)：versions.parse 拆回 version/distribution/javafx → 各发行商 API
-- ctx = { version, distribution, os, arch, ext }（宿主注入）
-- ---------------------------------------------------------------------------
function pre_install(ctx)
  local p = versions.parse(ctx.version)
  local dist = p.distribution or ctx.distribution or "open"
  local major = tostring(p.version):match("^(%d+)")
  if not major then
    return nil
  end
  if dist == "open" or dist == "tem" then
    return adoptium_package(ctx, major, p.javafx)
  elseif dist == "zulu" then
    return zulu_package(ctx, major, p.javafx)
  elseif dist == "librca" then
    return liberica_package(ctx, major, p.javafx)
  elseif dist == "corretto" or dist == "bisheng" or dist == "albba" or dist == "graal" then
    return github_package(ctx, dist, major)
  end
  return nil
end

-- ---------------------------------------------------------------------------
-- Adoptium（open / tem）：assets/latest/{major}/hotspot（已验证的方式）
-- image_type=jdk / jfx 区分是否含 JavaFX；release_name 即解压根目录
-- ---------------------------------------------------------------------------
function adoptium_package(ctx, major, javafx)
  local os_map = { win = "windows", linux = "linux", darwin = "mac" }
  local os = os_map[ctx.os] or "linux"
  local image_type = javafx and "jfx" or "jdk"
  local api = "https://api.adoptium.net/v3/assets/latest/" .. major
    .. "/hotspot?os=" .. os .. "&architecture=" .. ctx.arch .. "&image_type=" .. image_type
  local assets = json.decode(http.get(api))
  if type(assets) ~= "table" or #assets == 0 then
    return nil
  end
  local pkg = assets[1].binary.package
  return {
    url = pkg.link,
    file_name = pkg.name,
    checksum = pkg.checksum and ("sha256:" .. pkg.checksum) or nil,
    root_hint = assets[1].release_name,
  }
end

-- ---------------------------------------------------------------------------
-- Azul Zulu：api.azul.com/metadata/v1/zulu/packages/
-- FX 变体 = java_package_type=jdk-fx
-- ---------------------------------------------------------------------------
function zulu_package(ctx, major, javafx)
  local os_map = { win = "windows", linux = "linux", darwin = "macos" }
  local arch_map = { x64 = "x86_64", arm64 = "aarch64" }
  local os = os_map[ctx.os] or "linux"
  local arch = arch_map[ctx.arch] or "x86_64"
  local archive = (ctx.ext == "zip") and "zip" or "tar.gz"
  local api = "https://api.azul.com/metadata/v1/zulu/packages/"
    .. "?java_version=" .. major
    .. "&os=" .. os .. "&arch=" .. arch
    .. "&archive_type=" .. archive
    .. "&java_package_type=" .. (javafx and "jdk-fx" or "jdk")
    .. "&release_status=ga&latest=true&page=1&page_size=1"
  local list = json.decode(http.get(api))
  if type(list) ~= "table" or #list == 0 then
    return nil
  end
  local pkg = list[1]
  return {
    url = pkg.download_url,
    file_name = pkg.name,
    checksum = pkg.sha256_hash and ("sha256:" .. pkg.sha256_hash) or nil,
  }
end

-- ---------------------------------------------------------------------------
-- Liberica（BellSoft）：api.bell-sw.com/v1/liberica/releases
-- FX 变体 = bundle-type=jdk-fx
-- ---------------------------------------------------------------------------
function liberica_package(ctx, major, javafx)
  local os_map = { win = "windows", linux = "linux", darwin = "macos" }
  local arch_map = { x64 = "amd64", arm64 = "aarch64" }
  local os = os_map[ctx.os] or "linux"
  local arch = arch_map[ctx.arch] or "amd64"
  local pkg_type = (ctx.ext == "zip") and "zip" or "tar.gz"
  local api = "https://api.bell-sw.com/v1/liberica/releases"
    .. "?version-feature=" .. major
    .. "&os=" .. os .. "&arch=" .. arch
    .. "&bitness=64&package-type=" .. pkg_type
    .. "&bundle-type=" .. (javafx and "jdk-fx" or "jdk")
    .. "&release-type=all&latest=true"
  local list = json.decode(http.get(api))
  if type(list) ~= "table" or #list == 0 then
    return nil
  end
  local rel = list[1]
  return {
    url = rel.downloadUrl,
    file_name = rel.filename,
    checksum = rel.checksum and ("sha256:" .. rel.checksum) or nil,
  }
end

-- ---------------------------------------------------------------------------
-- GitHub Releases 系（corretto / bisheng / albba / graal）：
-- releases/latest → 按平台关键字匹配资产名（GitHub 系无 JavaFX 构建）
-- 注意：api.github.com 未认证限流 60 次/小时，刷新过频可能失败（插件层可重试）。
-- ---------------------------------------------------------------------------
function github_package(ctx, dist, major)
  local repo = GITHUB_REPO[dist](major)
  local os_kw = (ctx.os == "win") and "windows" or ((ctx.os == "darwin") and "macos" or "linux")
  local arch_kw = (ctx.arch == "arm64") and "aarch64" or "x64"
  local ext = (ctx.ext == "zip") and ".zip" or ".tar.gz"
  local body = json.decode(http.get("https://api.github.com/repos/" .. repo .. "/releases/latest"))
  local tag = body.tag_name or ""
  local assets = body.assets or {}
  local best
  for _, a in ipairs(assets) do
    local n = a.name:lower()
    local has = function(kw) return n:find(kw, 1, true) ~= nil end
    if has(os_kw) and has(arch_kw) and has(ext)
      and not has("source") and not has("debug") and not has("sources") then
      best = a
      break
    end
  end
  if not best then
    return nil
  end
  local version_tag = tag:match("[%d%.]+") or major
  return {
    url = best.browser_download_url,
    file_name = best.name,
    root_hint = "jdk-" .. version_tag,
  }
end

-- ---------------------------------------------------------------------------
-- post_install(ctx)：解压完成、链接 current 之前调用（可选 hook）
-- macOS：Adoptium/Temurin 解压出 jdk-<v>.jdk/Contents/Home/* —— 把 Home 内容上移到 root
-- （可信沙箱放开 os，可直接 os.execute；失败仅告警，不阻断安装）
-- ---------------------------------------------------------------------------
function post_install(ctx)
  if ctx.os ~= "darwin" then
    return
  end
  local quoted = "'" .. ctx.root:gsub("'", "'\\''") .. "'"
  os.execute('sh -c "cd ' .. quoted .. ' && [ -d Contents/Home ] && cp -R Contents/Home/. . && rm -rf Contents"')
end

-- ---------------------------------------------------------------------------
-- env_keys(ctx)：JAVA_HOME + bin PATH
-- ---------------------------------------------------------------------------
function env_keys(ctx)
  return { vars = { JAVA_HOME = ctx.root }, paths = { ctx.root .. "/bin" } }
end
