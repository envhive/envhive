//! 蜂巢 EnvHive —— Tauri 桌面入口与命令注册
//!
//! P0·地基（roadmap B/F）：三作用域配置链、Envs 合并、Session 临时目录、跨平台链接、
//! 版本解析工具等骨架 API 为 P1 预留，当前 command 层尚未全部消费，统一抑制 dead_code 警告。

#![allow(dead_code)]

mod commands;

// envhive-core 重导出：保持 crate::config / crate::error / crate::pathmeta /
// crate::toml_chain / crate::util / crate::logging / crate::env 路径可用
pub use envhive_core::{config, env, error, logging, pathmeta, toml_chain, util};
// envhive-toolkit 重导出：保持 crate::tool / crate::plugin / crate::lua_plugin /
// crate::registry / crate::mirror 路径可用
pub use envhive_toolkit::{lua_plugin, mirror, plugin, registry, tool};
// envhive-manager 重导出：保持 crate::manager / crate::queue / crate::usage /
// crate::autostart / crate::projects / crate::extras / crate::events 路径可用
pub use envhive_manager::{autostart, events, extras, manager, projects, queue, usage};
// bail! 宏重导出
pub use envhive_core::bail;

use std::sync::Arc;

use tauri::Manager;

use crate::commands::AppState;
use crate::config::AppConfig;
use crate::manager::EnvHiveManager;
use crate::pathmeta::PathMeta;
use crate::queue::QueueManager;

/// 构建 HTTP 客户端（代理第 1 层：蜂巢下载代理；仅应用内生效）
fn build_http_client(cfg: &AppConfig) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .user_agent("envhive/0.1.0")
        .connect_timeout(std::time::Duration::from_secs(15));
    if cfg.proxy.enable {
        if let Some(url) = &cfg.proxy.url {
            match reqwest::Proxy::all(url) {
                Ok(proxy) => {
                    tracing::info!("启用下载代理: {url}");
                    builder = builder.proxy(proxy);
                }
                Err(e) => tracing::warn!("代理地址无效 {url}: {e}"),
            }
        }
    }
    builder.build().unwrap_or_default()
}

/// 系统托盘（roadmap P1·体验）：托盘图标 + 右键菜单（打开主界面 / 退出）
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(app, "show", "打开主界面", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::with_id("envhive-tray")
        .icon(app.default_window_icon().cloned().unwrap_or_else(|| tauri::image::Image::new(&[], 0, 0)))
        .tooltip("蜂巢 EnvHive")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } =
                event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;
    tracing::info!("系统托盘已创建");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 1. 数据目录迁移（一次性，品牌重命名）：~/.envhive → ~/.envhive
    //    - 旧目录存在且新目录不存在 → 整体 rename（同盘原子移动，插件 / 已装工具 / 配置全部保留）
    //    - 迁移成功后改写 config.yaml 内残留的旧路径（.envhive → .envhive）
    //    - rename 失败（如目录被占用）→ 回退继续使用旧目录，保证已装工具不丢
    let home = dirs::home_dir().expect("无法确定用户主目录");
    let legacy_root = home.join(".envhive");
    let new_root = home.join(".envhive");
    let root_dir = if legacy_root.exists() && !new_root.exists() {
        match std::fs::rename(&legacy_root, &new_root) {
            Ok(()) => {
                eprintln!("[envhive] 数据目录已迁移: ~/.envhive → ~/.envhive");
                // 目录整体迁移后，内部的旧文件名（.envhive.toml）跟随搬入，需改名
                let legacy_global = new_root.join(".envhive.toml");
                if legacy_global.exists() {
                    let _ = std::fs::rename(&legacy_global, new_root.join(".envhive.toml"));
                }
                // config.yaml 内旧路径引用（tool_path 等）统一改写
                let cfg_file = new_root.join("config.yaml");
                if let Ok(content) = std::fs::read_to_string(&cfg_file) {
                    if content.contains(".envhive") {
                        let _ = std::fs::write(&cfg_file, content.replace(".envhive", ".envhive"));
                    }
                }
                new_root
            }
            Err(e) => {
                eprintln!("[envhive] 数据目录迁移失败（{e}），本次回退使用 ~/.envhive 继续");
                legacy_root
            }
        }
    } else {
        new_root
    };
    let config_path = root_dir.join("config.yaml");

    // 配置损坏时回退默认并重命名坏文件，保证应用可启动
    let (config, config_warn) = match AppConfig::load(&config_path) {
        Ok(cfg) => (cfg, None),
        Err(e) => {
            eprintln!("[envhive] config.yaml 加载失败，使用默认配置: {e}");
            let _ = std::fs::rename(&config_path, root_dir.join("config.yaml.bak"));
            (AppConfig::default(), Some(e.to_string()))
        }
    };

    // 2. 目录初始化（无网络依赖）
    let paths = match PathMeta::init(&root_dir, &config) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[envhive] 目录初始化失败: {e}");
            std::process::exit(1);
        }
    };

    // 3. 日志系统
    logging::init(&paths.logs);

    // 4. 启动写权限校验（roadmap I）
    if let Err(e) = config.verify_writable() {
        tracing::error!("工具 存储目录不可写: {e}");
    }
    if let Some(w) = config_warn {
        tracing::warn!("config.yaml 已回退默认并备份: {w}");
    }

    // 5. 写回默认配置（首次运行生成 config.yaml）
    if !config_path.exists() {
        if let Err(e) = config.save(&config_path) {
            tracing::warn!("生成 config.yaml 失败: {e}");
        }
    }

    // 6. Manager + 队列
    let client = build_http_client(&config);
    // 插件同步需要的数据（config 随后移入 manager，先取引用）
    let reg_addrs = config.registry_addresses();
    let manager = EnvHiveManager::new(config, paths, client);
    let sync_paths = manager.paths.clone();
    let sync_client = manager.client();
    let queue = Arc::new(QueueManager::new());

    tauri::Builder::default()
        // 单实例锁：同一时刻只允许一个 envhive.exe 进程运行。
        // 二次启动时，将已运行实例的主窗口唤起到前台并退出新进程。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
            tracing::info!("检测到重复启动，已唤起主窗口（本实例退出）");
        }))
        // 应用内更新：检查 / 下载 / 安装由前端 @tauri-apps/plugin-updater 驱动
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // 系统浏览器打开外部链接（关于页仓库地址等）
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { manager: Arc::new(manager), queue })
        .invoke_handler(tauri::generate_handler![
            commands::list_tools,
            commands::get_tool,
            commands::get_versions,
            commands::search_versions,
            commands::install_tool,
            commands::enqueue_install,
            commands::queue_status,
            commands::cancel_task,
            commands::queue_cancel_all,
            commands::queue_clear_finished,
            commands::uninstall_tool,
            commands::switch_version,
            commands::unuse_global,
            commands::current_tool,
            commands::list_installed,
            commands::get_config,
            commands::update_config,
            commands::set_registry_selected,
            commands::get_logs,
            commands::get_app_paths,
            commands::bootstrap,
            // P1·体验：镜像源 / proxy
            commands::list_registry_presets,
            commands::add_custom_registry_preset,
            commands::remove_custom_registry_preset,
            commands::get_registry_state,
            commands::apply_registry,
            commands::get_proxy,
            commands::set_proxy,
            // P1·开放：插件 / 进程注入 / 环境预览
            commands::list_plugins,
            commands::preview_env,
            commands::spawn_with_env,
            // P1 收尾：冲突检测 / 导入导出 / 远程注册表
            commands::check_conflicts,
            commands::ack_registry_conflict,
            commands::export_env,
            commands::import_env,
            commands::list_remote_plugins,
            commands::install_remote_plugin,
            // P2 · 增殖：开机自启动 / 托盘常驻 / 加速镜像 / 使用统计 / Lua 插件
            commands::get_autostart,
            commands::set_autostart,
            commands::get_tray_resident,
            commands::set_tray_resident,
            commands::get_download_mirror,
            commands::set_download_mirror,
            commands::set_tool_mirror,
            commands::get_usage_stats,
            commands::add_lua_plugin,
            commands::toggle_plugin,
            commands::delete_plugin,
            commands::open_plugin_dir,
            commands::read_plugin_source,
            commands::save_plugin_source,
            // 首页总览：当前系统使用的 工具 及其注入的环境变量
            commands::home_overview,
            // UI v2：应用为系统环境变量 / 项目预设 / 会话启动 / 日志查看器
            commands::apply_global_env,
            commands::list_projects,
            commands::save_project,
            commands::delete_project,
            commands::launch_session,
            commands::list_logs,
            commands::read_logs,
        ])
        .setup(move |app| {
            tracing::info!("envhive v{} 启动（平台 {} {}）", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
            // P2：开机自启动 --autostart 参数 → 隐藏主窗口（常驻托盘）
            let args: Vec<String> = std::env::args().collect();
            if args.iter().any(|a| a == "--autostart") {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.hide();
                }
                tracing::info!("以自启动模式启动（窗口已隐藏，托盘常驻）");
            }
            // 托盘常驻：关闭窗口 → 隐藏到托盘（后台继续运行）
            // 运行期每次 close 实时读取配置，开关切换即时生效，无需重启。
            if let Some(win) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                let w = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let resident = app_handle.state::<AppState>().manager.tray_resident();
                        if resident {
                            api.prevent_close();
                            let _ = w.hide();
                            tracing::info!("关闭窗口已拦截：隐藏到托盘（托盘常驻）");
                        }
                    }
                });
            }
            // 系统托盘
            if let Err(e) = setup_tray(app) {
                tracing::warn!("系统托盘创建失败: {e}");
            }
            // 插件同步：应用不再内置插件，全部从 Git 仓库（Gitee/GitHub raw 直链）下载。
            // 后台拉取 manifest → 安装缺失插件（已存在 / 已禁用的不覆盖），保证首启开箱可用。
            let (sp, sc, sa) = (sync_paths.clone(), sync_client.clone(), reg_addrs.clone());
            tauri::async_runtime::spawn(async move {
                crate::extras::sync_remote_plugins(&sp, &sa, &sc).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
