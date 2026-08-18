-- 蜂巢内置默认插件：Maven（Apache Maven 官方分发）
--
-- 版本列表：拉取 Maven Central 的 apache-maven 工件元数据（maven 每次发布都会同步该工件），
--   过滤 alpha/beta/rc/SNAPSHOT 只保留 GA，按 major.minor 系列取最新若干条；
--   网络失败时直接报错（不返回固定兜底列表）。
-- 下载：优先 dlcdn.apache.org（当前分发、速度快、可走镜像）；旧版本 dlcdn 已清理（404）
--   → 自动回退 archive.apache.org（全量历史、永久保留）；镜像下载 404 时宿主自动回退官方源。
-- 环境注入：MAVEN_HOME + bin PATH（mvn 3.9+/4.x 优先读 MAVEN_HOME，M2_HOME 已弃用）。

TOOL = {
  name = "maven",
  display = "Maven",
  category = "build",
  homepage = "https://maven.apache.org",
  version = "1.0.0",
  verify_bin = "bin/mvn",
  verify_arg = "-version",
  bin_suffix = "/bin",
  env_vars = { MAVEN_HOME = "{root}" },

  -- 下载加速镜像候选：镜像同步 dlcdn（仅当前发行版），旧版本仍回退官方 archive
  -- archive 国内镜像同步通常滞后 / 不全；旧版本可能仍走 archive 官方
  mirrors = {
    { name = "阿里云", from = "https://dlcdn.apache.org/maven/", to = "https://mirrors.aliyun.com/apache/maven/" },
    { name = "清华",   from = "https://dlcdn.apache.org/maven/", to = "https://mirrors.tuna.tsinghua.edu.cn/apache/maven/" },
    { name = "阿里云", from = "https://archive.apache.org/dist/maven/", to = "https://mirrors.aliyun.com/apache/maven/" },
    { name = "清华",   from = "https://archive.apache.org/dist/maven/", to = "https://mirrors.tuna.tsinghua.edu.cn/apache/maven/" },
  },
  default_mirror = "阿里云",
}

NETWORK_ALLOW = {
  "repo.maven.apache.org", "dlcdn.apache.org", "archive.apache.org",
}

-- Maven Central 上 apache-maven 工件的元数据（含全部历史版本，含 alpha/beta/rc）
local METADATA_URL = "https://repo.maven.apache.org/maven2/org/apache/maven/apache-maven/maven-metadata.xml"

-- ---------------------------------------------------------------------------
-- available()：Maven Central 元数据 → GA 版本，按 major.minor 系列取最新若干条
-- 策略：最新 5 个系列；最新系列取 6 条，其余系列取 3 条（总 ~18 条）
-- 网络失败直接报错（不返回固定兜底列表，避免展示过时/虚假的可用版本）
-- ---------------------------------------------------------------------------
function available()
  local ok, body = pcall(http.get, METADATA_URL)
  if not ok then
    error("获取 Maven 版本列表失败（网络不可用）：" .. tostring(body))
  end
  -- 收集 GA 版本并按 major.minor 分组（元数据顺序：旧 → 新）
  local groups = {}
  for v in body:gmatch("<version>([^<]+)</version>") do
    local vlow = v:lower()
    if not (vlow:find("alpha") or vlow:find("beta") or vlow:find("-rc") or vlow:find("snapshot")) then
      local major, minor = v:match("^(%d+)%.(%d+)")
      if major and tonumber(major) >= 3 then
        local key = major .. "." .. minor
        groups[key] = groups[key] or {}
        table.insert(groups[key], v)
      end
    end
  end
  -- 系列按数字排序（新 → 旧），同系列内版本也按数字排序（新 → 旧）
  local keys = {}
  for k in pairs(groups) do
    table.insert(keys, k)
  end
  local function to_num(k)
    local a, b = k:match("^(%d+)%.(%d+)")
    return tonumber(a), tonumber(b)
  end
  table.sort(keys, function(a, b)
    local amaj, amin = to_num(a)
    local bmaj, bmin = to_num(b)
    if amaj ~= bmaj then return amaj > bmaj end
    return amin > bmin
  end)
  -- 组装输出：最新系列 6 条，其余系列 3 条
  local out = {}
  for i, k in ipairs(keys) do
    if i > 5 then break end
    local list = groups[k]
    table.sort(list, function(a, b)
      local amaj, amin, apat = a:match("^(%d+)%.(%d+)%.([%d]+)")
      local bmaj, bmin, bpat = b:match("^(%d+)%.(%d+)%.([%d]+)")
      if tonumber(amaj) ~= tonumber(bmaj) then return tonumber(amaj) > tonumber(bmaj) end
      if tonumber(amin) ~= tonumber(bmin) then return tonumber(amin) > tonumber(bmin) end
      return tonumber(apat or 0) > tonumber(bpat or 0)
    end)
    local keep = (i == 1) and 6 or 3
    for j = 1, math.min(keep, #list) do
      table.insert(out, { version = list[j], labels = { "stable" } })
    end
  end
  return out
end

-- ---------------------------------------------------------------------------
-- pre_install(ctx)：构造下载地址
-- 4.x 系列在 maven-4/ 目录，其余在 maven-3/
-- dlcdn 仅保留当前分发版本：http.head 探测 tar.gz（200 → 走 dlcdn/镜像，404 → 回退 archive）。
-- 校验：maven 官方只分发 .sha512（宿主支持 "sha512:" 前缀），经 dlcdn 请求会 302 到 archive。
-- ---------------------------------------------------------------------------
function pre_install(ctx)
  local v = ctx.version
  local ext = ctx.ext or "tar.gz"
  local base = "apache-maven-" .. v
  local series = (v:match("^(%d+)") == "4") and "maven-4" or "maven-3"
  local dlcdn = "https://dlcdn.apache.org/maven/" .. series .. "/" .. v
    .. "/binaries/" .. base .. "-bin." .. ext
  local url = dlcdn
  local ok, code = pcall(http.head, dlcdn)
  if not (ok and code == 200) then
    url = "https://archive.apache.org/dist/maven/" .. series .. "/" .. v
      .. "/binaries/" .. base .. "-bin." .. ext
  end
  local checksum
  local ok2, sha = pcall(http.get, url .. ".sha512")
  if ok2 then
    local hex = sha:match("[%x]+")
    if hex then checksum = "sha512:" .. hex end
  end
  return { url = url, file_name = base .. "-bin." .. ext, checksum = checksum, root_hint = base }
end

-- ---------------------------------------------------------------------------
-- env_keys(ctx)：MAVEN_HOME + bin PATH
-- ---------------------------------------------------------------------------
function env_keys(ctx)
  return { vars = { MAVEN_HOME = ctx.root }, paths = { ctx.root .. "/bin" } }
end
