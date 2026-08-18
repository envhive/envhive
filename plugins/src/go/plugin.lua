-- 蜂巢内置默认插件：Go（go.dev 官方分发，URL 直接构造 + .sha256 校验）
TOOL = {
  name = "go",
  display = "Go",
  category = "language",
  homepage = "https://go.dev",
  version = "1.0.0",
  verify_bin = "bin/go",
  verify_arg = "version",
  bin_suffix = "/bin",
  root_hint = "go",             -- go 压缩包解压出 go/ 目录

  -- 下载加速镜像候选：dl.google.com 二进制分发（含 .sha256）在部分网络不可达，
  -- 前端可切换；规则 = from 前缀命中即替换为 to（pre_install 内 http.get 的 .sha256 同源）。
  mirrors = {
    { name = "阿里云", from = "https://dl.google.com/go/", to = "https://mirrors.aliyun.com/golang/" },
    { name = "清华",   from = "https://dl.google.com/go/", to = "https://mirrors.tuna.tsinghua.edu.cn/golang/" },
  },
  default_mirror = "阿里云",
}

NETWORK_ALLOW = { "go.dev", "dl.google.com" }

-- 版本列表：go.dev/dl JSON API（全部历史 stable 版本，去掉 "go" 前缀）
function available()
  --https://go.dev/dl/?mode=json&include=all
  local list = json.decode(http.get("https://go.dev/dl/?mode=json"))
  local out = {}
  for _, item in ipairs(list) do
    local v = item.version
    if v:sub(1, 2) == "go" then v = v:sub(3) end
    table.insert(out, { version = v, labels = { "stable" } })
  end
  return out
end

-- 下载：dl.google.com/go/go{version}.{os}-{arch}.{ext}（官方目录命名）
function pre_install(ctx)
  local os_map = { win = "windows", linux = "linux", darwin = "darwin" }
  local arch_map = { x64 = "amd64", arm64 = "arm64" }
  local os = os_map[ctx.os] or ctx.os
  local arch = arch_map[ctx.arch] or ctx.arch
  local base = "go" .. ctx.version .. "." .. os .. "-" .. arch
  local url = "https://dl.google.com/go/" .. base .. "." .. ctx.ext
  local checksum
  local ok, body = pcall(http.get, url .. ".sha256")
  if ok then checksum = "sha256:" .. body end
  return { url = url, file_name = base .. "." .. ctx.ext, checksum = checksum, root_hint = "go" }
end

-- 环境注入：GOROOT 指向 current 链接（覆盖继承的残留值，如其他版本管理器遗留的 GOROOT），
-- PATH 前置 bin 目录
function env_keys(ctx)
  return { vars = { GOROOT = ctx.root }, paths = { ctx.root .. "/bin" } }
end
