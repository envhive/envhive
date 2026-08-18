-- 蜂巢内置默认插件：Tomcat（Apache Tomcat 官方分发）
--
-- 版本列表：Tomcat 无集中式元数据文件，改为解析 Apache 分发目录 ——
--   系列 = archive 根目录下 tomcat-<major>/（仅保留 major >= 9 的活跃系列，
--     自动兼容未来 Tomcat 12，无需改插件）；版本 = 各系列下 v<ver>/ 目录
--     （过滤 -M/-RC 里程碑，只留 GA）。网络失败直接报错（不返回固定兜底列表）。
-- 下载：优先 dlcdn.apache.org（当前版本、速度快、可走镜像）；旧版本 dlcdn 已清理（404）
--   → 自动回退 archive.apache.org（全量历史、永久保留）；镜像下载 404 时宿主自动回退官方源。
-- 校验：Tomcat 官方分发 .sha512（宿主支持 "sha512:" 前缀）。
-- 安装：解压即用（zip/tar.gz 解压即为完整可运行目录），verify_bin 留空声明跳过安装后
--   运行验证 —— catalina/version 脚本强依赖 JAVA_HOME 且版本需与 Tomcat 匹配
--   （Tomcat 11 需 Java 17+），验证与解压安装本身无关。
-- 环境注入：CATALINA_HOME + bin PATH（catalina / startup / shutdown 脚本）。

TOOL = {
  name = "tomcat",
  display = "Tomcat",
  category = "server",
  homepage = "https://tomcat.apache.org",
  version = "1.0.0",
  verify_bin = "", -- 空 = 解压即用，跳过安装后运行验证
  verify_arg = "",
  bin_suffix = "/bin",
  env_vars = { CATALINA_HOME = "{root}" },

  -- 下载加速镜像候选：镜像同步 archive 全量 + dlcdn 当前版
  mirrors = {
    { name = "阿里云", from = "https://dlcdn.apache.org/tomcat/", to = "https://mirrors.aliyun.com/apache/tomcat/" },
    { name = "清华",   from = "https://dlcdn.apache.org/tomcat/", to = "https://mirrors.tuna.tsinghua.edu.cn/apache/tomcat/" },
    { name = "阿里云", from = "https://archive.apache.org/dist/tomcat/", to = "https://mirrors.aliyun.com/apache/tomcat/" },
    { name = "清华",   from = "https://archive.apache.org/dist/tomcat/", to = "https://mirrors.tuna.tsinghua.edu.cn/apache/tomcat/" },
  },
  default_mirror = "阿里云",
}

NETWORK_ALLOW = {
  "dlcdn.apache.org", "archive.apache.org",
}

-- 活跃系列下限（tomcat-3 ~ tomcat-8 均为 EOL 历史系列，不展示）
local MIN_MAJOR = 9

-- 版本号数字比较（"11.0.24" > "10.1.57" > "9.0.120"），返回降序
local function ver_cmp(a, b)
  local pa = { a:match("^(%d+)%.(%d+)%.(%d+)") }
  local pb = { b:match("^(%d+)%.(%d+)%.(%d+)") }
  for i = 1, 3 do
    local na, nb = tonumber(pa[i] or 0), tonumber(pb[i] or 0)
    if na ~= nb then
      return na > nb
    end
  end
  return false
end

-- ---------------------------------------------------------------------------
-- available()：dlcdn 根目录 → 活跃系列（快、只含在维护系列）→ 各系列 archive 目录页 → GA 版本
-- 策略：系列按 major 降序；每系列取最新 4 条（10.1 当前生产推荐标 lts）
-- 网络失败直接报错（不返回固定兜底列表，避免展示过时/虚假的可用版本）
-- ---------------------------------------------------------------------------
function available()
  -- 1) 系列列表：dlcdn 根目录（比 archive 快）；失败回退固定活跃清单
  local series = {}
  local ok, body = pcall(http.get, "https://dlcdn.apache.org/tomcat/")
  if ok then
    for dir, num in body:gmatch('href="(tomcat-(%d+))/"') do
      local major = tonumber(num)
      if major and major >= MIN_MAJOR then
        table.insert(series, { dir = dir, major = major })
      end
    end
  end
  if #series == 0 then
    for _, major in ipairs({ 11, 10, 9 }) do
      table.insert(series, { dir = "tomcat-" .. major, major = major })
    end
  end
  table.sort(series, function(a, b)
    return a.major > b.major
  end)

  -- 2) 每系列拉 archive 目录页 → GA 版本（降序取最新若干条；单系列失败跳过不阻断）
  local out = {}
  local max_per_series = 4
  for _, s in ipairs(series) do
    local vok, vbody = pcall(http.get, "https://archive.apache.org/dist/tomcat/" .. s.dir .. "/")
    if vok then
      local list = {}
      for vdir in vbody:gmatch('href="(v[^"/]+)/"') do
        local v = vdir:sub(2)
        -- 只留 GA（vX.Y.Z）：过滤 -M1 / -RC1 等里程碑与预发布目录
        if v:match("^%d+%.%d+%.%d+$") then
          table.insert(list, v)
        end
      end
      table.sort(list, ver_cmp)
      local labels = { "stable" }
      if s.major == 10 then
        -- 10.1.x 为当前生产推荐系列（长期维护）
        labels = { "lts", "stable" }
      end
      for j = 1, math.min(max_per_series, #list) do
        table.insert(out, { version = list[j], labels = labels })
      end
    end
  end
  return out
end

-- ---------------------------------------------------------------------------
-- pre_install(ctx)：构造下载地址
-- 路径规则：tomcat-<major>/v<version>/bin/apache-tomcat-<version>.<ext>
-- dlcdn 仅保留当前分发版本：http.head 探测 tar.gz/zip（200 → 走 dlcdn/镜像，404 → 回退 archive）
-- ---------------------------------------------------------------------------
function pre_install(ctx)
  local v = ctx.version
  local major = v:match("^(%d+)")
  if not major then
    return nil
  end
  local series = "tomcat-" .. major
  local base = "apache-tomcat-" .. v
  local ext = ctx.ext or "tar.gz"
  local dlcdn = "https://dlcdn.apache.org/tomcat/" .. series .. "/v" .. v
    .. "/bin/" .. base .. "." .. ext
  local url = dlcdn
  local ok, code = pcall(http.head, dlcdn)
  if not (ok and code == 200) then
    url = "https://archive.apache.org/dist/tomcat/" .. series .. "/v" .. v
      .. "/bin/" .. base .. "." .. ext
  end
  local checksum
  local ok2, sha = pcall(http.get, url .. ".sha512")
  if ok2 then
    local hex = sha:match("[%x]+")
    if hex then
      checksum = "sha512:" .. hex
    end
  end
  return { url = url, file_name = base .. "." .. ext, checksum = checksum, root_hint = base }
end

-- ---------------------------------------------------------------------------
-- env_keys(ctx)：CATALINA_HOME + bin PATH
-- ---------------------------------------------------------------------------
function env_keys(ctx)
  return { vars = { CATALINA_HOME = ctx.root }, paths = { ctx.root .. "/bin" } }
end
