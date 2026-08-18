-- 蜂巢内置默认插件：Rust（rustup 同款机制：channel manifest 驱动 + 动态 target + 官方 sha256 校验）
-- 版本粒度：stable / beta / nightly 三通道（各自最新），并支持任意具体版本号
-- （pre_install 按 rustup 规则请求 channel-rust-{version}.toml，版本不存在时由宿主报错）。
TOOL = {
  name = "rust",
  display = "Rust",
  category = "language",
  homepage = "https://www.rust-lang.org",
  version = "1.1.0",
  verify_bin = "bin/rustc",
  verify_arg = "--version",
  bin_suffix = "/bin",

  -- 下载加速镜像候选：static.rust-lang.org 在部分网络不可达，前端可切换；
  -- rsproxy 完整同步 channel 与 dist（结构一致），清华/中科大镜像结构不同，暂不列入。
  mirrors = {
        { name = "rsproxy", from = "https://static.rust-lang.org/dist/", to = "https://rsproxy.cn/dist/" },
        { name = "中科大", from = "https://static.rust-lang.org/dist/", to = "https://mirrors.ustc.edu.cn/rust-static/rustup" },
  },
  default_mirror = "rsproxy",
}

NETWORK_ALLOW = { "static.rust-lang.org" }

-- 从 channel manifest 中提取 [pkg.<name>] 段的 version（如 "1.97.1 (8bab26f4f 2026-07-14)"），
-- 取首个空格前的纯版本号（nightly 形如 "1.99.0-nightly (abc123 2026-08-16)"）。
local function parse_pkg_version(body, name)
  local section
  for line in body:gmatch("[^\r\n]+") do
    local s = line:match("^%[([^%]]+)%]$")
    if s then
      section = s
    elseif section == name then
      local v = line:match('^version%s*=%s*"([^"]+)"')
      if v then return v:match("^([^ ]+)") or v end
    end
  end
  return nil
end

-- 版本号 → channel：关键词直用；"-nightly"/"-beta" 后缀归入对应通道；
-- 纯版本号（如 1.85.0）按 rustup 规则请求 channel-rust-{version}.toml。
local function resolve_channel(v)
  v = v or ""
  if v == "stable" or v == "beta" or v == "nightly" then return v end
  if v:match("%-nightly") then return "nightly" end
  if v:match("%-beta") then return "beta" end
  if v:match("^%d") then return v end
  return "stable"
end

-- 版本列表：stable / beta / nightly 三通道各自当前版本（解析各自 channel manifest 的 [pkg.rust]）
function available()
  local out = {}
  for _, ch in ipairs({ { "stable", "stable" }, { "beta", "beta" }, { "nightly", "nightly" } }) do
    local ok, body = pcall(http.get, "https://static.rust-lang.org/dist/channel-rust-" .. ch[1] .. ".toml")
    if ok and type(body) == "string" then
      local v = parse_pkg_version(body, "pkg.rust")
      if v then table.insert(out, { version = v, labels = { ch[2] } }) end
    end
  end
  return #out > 0 and out or nil
end

-- 下载：完全参照 rustup —— 抓取对应 channel manifest，从 [pkg.rust.target.<triple>]
-- 读取官方给出的 url / hash / available，避免硬编码目录规则（nightly 等命名差异自动兼容），
-- 并以官方 sha256 作为下载校验（checksum 字段，宿主下载后核验）。
function pre_install(ctx)
  local target_map = {
    ["win"] = { ["x64"] = "x86_64-pc-windows-msvc", ["arm64"] = "aarch64-pc-windows-msvc" },
    ["linux"] = { ["x64"] = "x86_64-unknown-linux-gnu", ["arm64"] = "aarch64-unknown-linux-gnu" },
    ["darwin"] = { ["x64"] = "x86_64-apple-darwin", ["arm64"] = "aarch64-apple-darwin" },
  }
  local t = target_map[ctx.os]
  local target = t and t[ctx.arch] or (ctx.os .. "-" .. ctx.arch)

  local body = http.get(
    "https://static.rust-lang.org/dist/channel-rust-" .. resolve_channel(ctx.version) .. ".toml"
  )

  -- 在 manifest 中定位 [pkg.rust.target.<triple>] 段，读 available / url / hash
  local want = "pkg.rust.target." .. target
  local section, avail, url, hash
  for line in body:gmatch("[^\r\n]+") do
    local s = line:match("^%[([^%]]+)%]$")
    if s then
      if section == want then break end -- 目标段已读完，停止
      section = s
    elseif section == want then
      local k, v = line:match('^([%w_]+)%s*=%s*"([^"]*)"')
      if k == "available" then avail = v
      elseif k == "url" then url = v
      elseif k == "hash" then hash = v end
    end
  end
  -- 该版本无此 target 构建（或版本不存在）→ 返回 nil，宿主报"该版本可能无对应构建"
  if not url or avail == "false" then return nil end

  local file_name = url:match("([^/]+)$") or "rust.tar.gz"
  local root_hint = file_name:gsub("%.tar%.gz$", "")
  return {
    url = url,
    file_name = file_name,
    checksum = hash and ("sha256:" .. hash) or nil,
    root_hint = root_hint,
  }
end

-- ============================================================
-- post_install：Rust 官方独立安装包不是"解压即用"布局——
-- tar.gz 解压后顶层是各组件目录（rustc/ cargo/ rust-std-<target>/
-- rustfmt-preview/ clippy-preview/ ...）+ install.sh，根目录下并没有
-- bin/rustc，直接验证必然失败。此 hook 在解压完成后、运行验证前执行：
-- 按 components 清单把每个组件的内容合并到安装根目录，形成标准的
-- bin/ + lib/rustlib/ 布局（等效 rustup 安装 toolchain 后的目录结构）。
-- Windows：robocopy /E /MOVE（退出码 0-7 = 成功）；Unix：cp -a + rm -rf。
-- 幂等：components 文件不存在（已合并过）则跳过。
-- 注意：hook 失败仅告警不中断安装，最终以 verify_bin 校验为准，
-- 因此本函数失败时不会阻止 bin/rustc 校验通过。
-- ============================================================
function post_install(ctx)
  local root = ctx.root
  local function log(s)
    local f = io.open(root .. "/post_install.log", "a")
    if f then f:write(s .. "\n") f:close() end
  end
  log("post_install start: " .. tostring(root))

  local comp_file = root .. "/components"
  if not file.exists(comp_file) then
    log("components 清单不存在，视为已合并，跳过")
    return
  end
  local body = file.read(comp_file) or ""
  local is_win = package.config:sub(1, 1) == "\\"
  local failed = false
  for line in body:gmatch("[^\r\n]+") do
    local c = line:gsub("%s+$", "")
    if c ~= "" and file.exists(root .. "/" .. c) then
      local src = root .. "/" .. c
      if is_win then
        -- cmd 下 robocopy 合并（无 MSYS 路径转换问题）；0-7 均为成功码
        local _, _, code = os.execute(
          'robocopy "' .. src .. '" "' .. root .. '" /E /MOVE /NFL /NDL /NJH /NJS /NP'
        )
        local good = code ~= nil and code >= 0 and code < 8
        log(string.format("merge %s -> good=%s (exit=%s)", c, tostring(good), tostring(code)))
        if not good then failed = true end
      else
        -- Unix：cp -a 合并目录内容（src/. → root/），完成后删除组件目录
        local ok = os.execute("cp -a '" .. src .. "/.' '" .. root .. "/' && rm -rf '" .. src .. "'")
        log(string.format("merge %s -> ok=%s", c, tostring(ok)))
        if ok ~= true then failed = true end
      end
    end
  end
  log("post_install done, failed=" .. tostring(failed))
end

function env_keys(ctx)
  return { paths = { ctx.root .. "/bin" } }
end
