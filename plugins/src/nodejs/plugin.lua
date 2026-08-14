-- 蜂巢内置默认插件：Node.js（nodejs.org 官方 dist，解压即用）
-- 可作为模板参考；修改本文件将覆盖内置行为，卸载后重新安装可恢复默认。
TOOL = {
  name = "nodejs",
  display = "Node.js",
  category = "language",
  homepage = "https://nodejs.org",
  version = "1.0.0",
  verify_bin = "node",
  verify_arg = "--version",
  bin_suffix = "",              -- node 可执行文件在解压根目录

  -- 下载加速镜像候选：前端据此渲染「加速镜像」下拉，多地址可切换；
  -- 规则 = from 前缀命中即替换为 to，作用于版本列表 / 下载包 / checksum 全链路。
  -- 官方源由宿主隐式提供（选择「官方」即不镜像），此处仅声明镜像候选。
  mirrors = {
    { name = "npmmirror", from = "https://nodejs.org/dist/", to = "https://npmmirror.com/mirrors/node/" },
    { name = "华为云",     from = "https://nodejs.org/dist/", to = "https://mirrors.huaweicloud.com/nodejs/" },
  },
  default_mirror = "npmmirror",
}

NETWORK_ALLOW = { "nodejs.org" }

-- 版本列表：官方 index.json（version 自带 "v" 前缀，剥离；含 lts 标记）
-- 宿主不再重排版本顺序（信任插件顺序），此处把官方"新在前"翻转为升序，保持 UI 展示习惯
function available()
  local list = json.decode(http.get("https://nodejs.org/dist/index.json"))
  local out = {}
  for _, item in ipairs(list) do
    local v = item.version
    if v:sub(1, 1) == "v" then v = v:sub(2) end
    local labels = { "stable" }
    if item.lts then labels = { "lts" } end
    table.insert(out, { version = v, labels = labels })
  end
  local asc = {}
  for i = #out, 1, -1 do
    table.insert(asc, out[i])
  end
  return asc
end

-- 下载地址：node-v{version}-{os}-{arch}.{ext}（官方目录命名与 ctx 别名一致）
function pre_install(ctx)
  local name = "node-v" .. ctx.version .. "-" .. ctx.os .. "-" .. ctx.arch
  local url = "https://nodejs.org/dist/v" .. ctx.version .. "/" .. name .. "." .. ctx.ext
  return { url = url, file_name = name .. "." .. ctx.ext }
end

function env_keys(ctx)
  return { paths = { ctx.root } }
end
