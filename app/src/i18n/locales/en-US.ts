// English (US) locale pack
// Mirrors the key structure of zh-CN.ts — the `typeof zhCN` annotation makes any
// missing / extra key a compile-time error, so the two packs can never drift apart.
// See docs/frontend-i18n-spec.md for the naming / interpolation conventions.
import type zhCN from "./zh-CN";

const enUS: typeof zhCN = {
  // ---------- App shell ----------
  app: {
    loading: {
      title: "EnvHive is starting",
      sub: "Loading environment info, mirror configuration and queue state…",
    },
  },

  // ---------- Navigation (shared by Sidebar / TopBar) ----------
  nav: {
    home: "Home",
    tools: "Tools",
    plugins: "Plugins",
    network: "Registries",
    settings: "Settings",
    stats: "Statistics",
    logs: "Logs",
    about: "About",
  },

  // ---------- Common actions & placeholders ----------
  common: {
    confirm: "Confirm",
    cancel: "Cancel",
    close: "Close",
    retry: "Retry",
    expand: "Expand",
    collapse: "Collapse",
    copy: "Copy",
    edit: "Edit",
    delete: "Delete",
    remove: "Remove",
    install: "Install",
    add: "Add",
    save: "Save",
    refresh: "Refresh",
    openDir: "Open folder",
    unknownError: "Unknown error",
    executing: "Running",
    executingEllipsis: "Running…",
    loadingEllipsis: "Loading…",
    noDescription: "(no description)",
    current: "Current",
    installed: "Installed",
    previewBanner: "Preview mode: UI only — the Rust backend is not connected.",
    toolIconAlt: "Tool icon",
  },

  // ---------- Relative time (types.ts relTime) ----------
  time: {
    justNow: "just now",
    minutesAgo: "{n} min ago",
    hoursAgo: "{n} h ago",
    daysAgo: "{n} d ago",
  },

  // ---------- Language switcher ----------
  language: {
    label: "Language",
  },

  // ---------- Top bar ----------
  topbar: {
    storageReadonly: "Storage not writable",
  },

  // ---------- Download / task stage (DownloadProgress.stage) ----------
  stage: {
    resolving: "Resolving",
    downloading: "Downloading",
    verifying: "Verifying",
    extracting: "Extracting",
    done: "Done",
    failed: "Failed",
  },

  // ---------- Task status (QueueTask.status) ----------
  task: {
    queued: "Queued",
    running: "Running",
    done: "Done",
    failed: "Failed",
    cancelled: "Cancelled",
  },

  // ---------- Download queue drawer ----------
  queue: {
    title: "Download queue",
    summary: "{total} tasks · {running} in progress",
    empty: "No tasks",
    emptyHint: "No tasks yet. Pick a version in Tools and click Install — the task will show up here.",
    cancelAll: "Cancel all",
    clearFinished: "Clear finished",
    cancelDownload: "Cancel download",
    cancel: "Cancel",
    retry: "Retry",
    urlCopied: "Download link copied",
    urlCopyFailed: "Copy failed, please select and copy manually",
    downloaded: "Downloaded {size}",
    remaining: "{time} left",
    enqueuedAt: "Queued {time}",
    runningAria: "Task in progress",
  },

  // ---------- Install modal ----------
  install: {
    title: "Install a new {name} version",
    distribution: "Distribution",
    pull: "Fetch",
    versionCount: "{count} versions",
    noCache: "No cache yet — click “Fetch” to load the version list",
    mirror: "Acceleration mirror",
    mirrorPlaceholder: "Select mirror…",
    officialSource: "Official source",
    noVersions: "No versions available — click “Fetch” to load from the version source",
    installing: "Installing…",
    selectVersion: "Select a version",
    versionInstalled: "This version is installed",
    installVersion: "Install {version}",
    close: "Close",
  },

  // ---------- Global toasts / system notifications ----------
  toast: {
    previewMode: "Preview mode: {msg}",
    logReadFailed: "Failed to read logs: {msg}",
    installCompleteTitle: "EnvHive · Install complete",
    installCompleteBody: "{tool} {version} installed",
    progressNote: "{tool} {version}: {note}",
    installFailed: "Install failed: {msg}",
    downloadFailed: "{tool} {version} download failed",
    versionsRefreshed: "{tool} version list refreshed ({count} versions)",
    versionsLoadFailed: "Failed to load versions for {tool}: {msg}",
    selectVersionFirst: "Please select a version for {tool} first",
    alreadyQueued: "{tool} {version} is already queued (task #{id}); no need to enqueue again",
    enqueued: "{tool} {version} added to queue (task #{id})",
    requeued: "{tool} {version} requeued (task #{id})",
    enqueueFailed: "Failed to enqueue: {msg}",
    retryFailed: "Retry failed: {msg}",
    switched: "{tool} → {version}: {message} (takes effect in new terminals; restart already-open windows)",
    switchFailed: "Switch failed: {msg}",
    notGlobalVersion: "{tool} is not set as a global version",
    globalCleared: "{tool} global setting cleared, reverted to system default",
    unuseFailed: "Failed to clear: {msg}",
    uninstalled: "{tool} {version} uninstalled",
    uninstallFailed: "Uninstall failed: {msg}",
    noUninstallableVersion: "No version to uninstall",
    mirrorApplied: "{tool} mirror switched to {name}",
    mirrorApplyFailed: "Failed to switch mirror: {msg}",
    customPresetAdded: "{tool} custom source “{name}” added",
    customPresetAddFailed: "Failed to add custom source: {msg}",
    customPresetRemoved: "{tool} custom source “{name}” removed",
    customPresetRemoveFailed: "Failed to remove custom source: {msg}",
    proxyEnabled: "Proxy enabled: {url}",
    proxyDisabled: "Proxy disabled",
    proxySaveFailed: "Failed to save proxy settings: {msg}",
    taskCancelled: "Task #{id} cancelled",
    cancelFailed: "Cancel failed: {msg}",
    cancelRequested: "Requested cancellation of {count} tasks",
    nothingToCancel: "No tasks to cancel",
    cancelAllFailed: "Cancel all failed: {msg}",
    clearedFinished: "Cleared {count} finished tasks",
    nothingToClear: "No finished tasks to clear",
    clearFailed: "Clear failed: {msg}",
    fingerprintRecorded: "{tool} fingerprint re-recorded",
    actionFailed: "Operation failed: {msg}",
    reswitched: "{tool} switched back to {version}",
    fixFailed: "Fix failed: {msg}",
    pluginUpdated: "Plugin {name} updated to v{version}",
    pluginInstalled: "Plugin {name} installed at v{version}",
    pluginInstallFailed: "Failed to install plugin: {msg}",
    pluginUpdateFailed: "Failed to update plugin: {msg}",
    autostartOn: "Launch at startup enabled (starts on boot with the window hidden in the tray)",
    autostartOff: "Launch at startup disabled",
    autostartFailed: "Failed to set launch at startup: {msg}",
    trayResidentOn:
      "Tray residency enabled: closing the window will hide it to the system tray and keep running in the background",
    trayResidentOff: "Tray residency disabled",
    trayResidentFailed: "Failed to set tray residency: {msg}",
    toolMirrorSet: "{tool} download mirror switched to “{mirror}”",
    toolMirrorReset: "{tool} reverted to the official source",
    toolMirrorFailed: "Failed to switch mirror: {msg}",
    luaFieldsRequired: "Please fill in the plugin name and script",
    luaAdded: "Lua plugin {name} added",
    luaAddFailed: "Failed to add Lua plugin: {msg}",
    scriptEmpty: "Script is empty",
    pluginSaved: "Plugin {name} saved",
    pluginSaveFailed: "Failed to save plugin: {msg}",
    pluginReadFailed: "Failed to read plugin: {msg}",
    pluginEnabled: "Plugin {name} enabled",
    pluginDisabled: "Plugin {name} disabled",
    pluginEnableFailed: "Failed to enable plugin: {msg}",
    pluginDisableFailed: "Failed to disable plugin: {msg}",
    pluginDeleted: "Plugin {name} deleted",
    pluginDeleteFailed: "Failed to delete plugin: {msg}",
    pluginDirFailed: "Failed to open plugin directory: {msg}",
    registryRequired: "At least one plugin registry address is required",
    configSaved: "Configuration saved (storage path changes take effect after restart)",
    configSaveFailed: "Failed to save configuration: {msg}",
    exportFailed: "Export failed: {msg}",
    importRequired: "Please paste the environment snapshot to import first",
    imported: "Environment snapshot imported and applied",
    importFailed: "Import failed: {msg}",
    projectSaved: "Project preset “{name}” saved",
    projectSaveFailed: "Failed to save project preset: {msg}",
    projectDeleted: "Project preset “{name}” deleted",
    projectDeleteFailed: "Failed to delete project preset: {msg}",
    sessionLaunched: "Launched {command} with preset “{name}”: a new window is open, check your taskbar",
    sessionLaunchFailed: "Launch failed: {msg}",
  },

  // ---------- Confirmation dialogs (title + content) ----------
  dialog: {
    unuseGlobal: {
      title: "Clear global setting for {tool}",
      content:
        "This deactivates {tool} {version} globally and removes it from PATH / {envName} and other environment variables (the version is not uninstalled). Takes effect in new terminals; restart already-open windows.",
    },
    uninstall: {
      title: "Uninstall {tool} {version}",
      content: "The installed version directory will be deleted. You can reinstall it later.",
      contentSelected:
        "Currently selected in the dropdown. The installed version directory will be deleted. You can reinstall it later.",
    },
    removeCustomPreset: {
      title: "Delete custom source “{name}”",
      content: "Delete custom source “{name}” for {tool}? (Written config files are not affected)",
    },
    cancelAll: {
      title: "Cancel all",
      content:
        "This cancels every queued task. Running tasks will stop as soon as they reach a download/verify checkpoint.",
    },
    ackConflict: {
      title: "Set the current configuration as baseline?",
      content:
        "{message}\n\nAfter confirming, the conflict notice is dismissed (it reappears if the config is modified externally again).",
    },
    installRemotePlugin: {
      titleUpdate: "Update plugin {name}: v{from} → v{to}",
      titleInstall: "Install remote plugin {name}@{version}",
    },
    deletePlugin: {
      title: "Delete plugin {name}",
      content:
        "This deletes the definition directory of plugin “{display}” ({name}).\n\nInstalled version files stay in the cache and can be cleaned up on the Statistics page.",
    },
    deleteProject: {
      title: "Delete project preset “{name}”",
      content: "The project directory itself is not affected.",
    },
  },

  // ---------- Home page ----------
  home: {
    desc: "Switching a tool writes to system variables (takes effect in new terminals; restart already-open windows). Project environments launch sessions with a preset version combo without writing system variables.",
    category: {
      language: "Language",
      build: "Build tool",
      tool: "Tool",
    },
    global: {
      title: "Global environment (system-wide)",
      summary: "{tools} active tools · {vars} env vars · {paths} PATH entries",
      emptyHint: "No global environment yet",
      emptyAction: "Go to Tools",
      noVersion: "No version configured",
      installedCount: "{count} versions installed",
      unuse: "Disable",
      unuseTip: "Remove this tool's environment variables",
      currentSuffix: "{version} · current",
      switchTip: "{count} versions installed — click a target version to switch (writes system variables, effective in new terminals)",
      onlyVersion: "Only version",
      switch: "Switch",
      envOverview: "Environment variables ({count})",
    },
    project: {
      title: "Project environments (session-level)",
      newPreset: "New preset",
      hint: "A project = directory + name + version combo. “Launch terminal” injects that combo's environment variables (JAVA_HOME, PATH point to the preset versions) without writing system variables; they disappear when the window closes.",
      emptyHint: "No project environment yet",
      emptyAction: "Create your first project environment",
      editTitle: "Edit preset “{name}”",
      newTitle: "New project preset",
      nameLabel: "Name",
      namePlaceholder: "e.g. order-service",
      dirLabel: "Directory",
      dirPlaceholder: "D:\\work\\order-service",
      save: "Save preset",
    },
    msg: {
      clonedGlobal: "Loaded the current global version combo",
      nameRequired: "Please enter a preset name",
      dirRequired: "Please enter a project directory",
      versionRequired:
        "Please pick an installed version for every tool (visit Tools to download if not installed)",
    },
  },

  // ---------- Tools page ----------
  tools: {
    listTitle: "Tool list ({count})",
    searchPlaceholder: "Search tools…",
    installedCount: "{count} installed",
    allCategories: "All",
    emptyAllHint: "All tools are disabled or uninstalled. Reinstall them from the plugin market.",
    emptyAllAction: "Go to plugin market",
    emptyFilterHint: "Try adjusting the filters or search keywords.",
  },

  // ---------- Tool card ----------
  toolCard: {
    pathMissing: "This tool is not on PATH",
    current: "Current",
    noGlobalVersion: "No global version set",
    installedVersions: "Installed versions",
    emptyVersions: "No versions installed yet — click “Install” to start",
    colVersion: "Version",
    colOps: "Actions",
    switchTitle: "Switch to this version (takes effect in new terminals)",
    switch: "Switch",
    uninstallTitle: "Uninstall this version (removes directory and environment variables)",
    uninstall: "Uninstall",
    unuseTitle: "Deactivate globally and remove from PATH / *_HOME (not uninstalled)",
    unuse: "Disable",
    expandMore: "Show {count} more versions",
  },

  // ---------- Project environment row ----------
  project: {
    emptyVersions: "No version combo configured",
    launchTerminal: "Launch terminal",
  },

  // ---------- Version combo picker ----------
  vcp: {
    title: "Version combo",
    cloneGlobal: "Clone from global",
    addTool: "+ Add tool",
    addTitleAllAdded: "All tools are already in the combo",
    addTitleSelect: "Select a tool to add",
    empty: "No tool selected yet — click “Clone from global” or “+ Add tool” to start.",
    toolPlaceholder: "Tool",
    distPlaceholder: "Distribution",
    noInstalledVersions: "No installed versions",
    selectVersion: "Select version",
    notInstalled: "{version} (not installed)",
    remove: "Remove",
  },

  // ---------- Env vars table ----------
  envTable: {
    key: "Variable",
    value: "Value",
    source: "Source",
  },

  // ---------- Settings page ----------
  settings: {
    general: "General",
    autostart: "Launch at startup",
    trayResident: "Hide to system tray when the window closes",
    proxy: "Proxy",
    downloadProxy: "Download proxy",
    proxyAddr: "Proxy address",
    storage: "Storage",
    cacheTtl: "Cache TTL",
    cacheTtlTip:
      "How long fetched version lists are cached. Supported formats: 12h (12 hours), 3600 (seconds), -1 (never expires), 0 (disable cache)",
    registries: "Plugin registries",
    registryNamePlaceholder: "Registry name (e.g. official-gitee)",
    registryUrlPlaceholder: "https://example.com/repo/main/manifest.json",
    registryUrlFullPlaceholder:
      "Full manifest.json URL (e.g. https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json)",
    deleteRepo: "Delete this registry",
    storagePath: "Tool storage path",
    storagePathWarning: "⚠️ Requires a restart to take effect",
    configFile: "Config file: {path}",
    dataMgmt: "Data management (import / export)",
    exportYaml: "Export YAML",
    exportJson: "Export JSON",
    exportHint: "Export a snapshot of global tool versions, mirrors, proxy and environment variables",
    copyExport: "Copy exported content",
    importPlaceholder:
      "Paste an exported environment snapshot (YAML / JSON). Importing writes global tool versions, mirrors, proxy and environment variables",
    importApply: "Import and apply",
    about: "About",
    aboutLine: "Version {version} · {platform}/{arch} · Config {writable} · Install dir {dir}",
    writable: "writable",
    notWritable: "not writable",
    msgCopied: "Copied to clipboard",
    msgCopyFailed: "Copy failed (unavailable in preview mode)",
    msgNameRequired: "Please enter a registry name (e.g. official-gitee)",
    msgUrlRequired:
      "Please enter the full manifest.json URL (e.g. https://raw.giteeusercontent.com/envhive/envhive/raw/main/plugins/manifest.json)",
    msgUrlExists: "This registry URL already exists",
    msgKeepOne: "Keep at least one plugin registry",
  },

  // ---------- Plugins page ----------
  plugins: {
    marketTitle: "Plugin market (remote registries)",
    loading: "Fetching plugin list…",
    loadFailed: "Failed to fetch plugin list: {msg}",
    emptyMarket: "No plugins available in this registry (configure more registries in Settings).",
    notInstalled: "Not installed",
    updatable: "Update available",
    updatableTip: "The installed version differs from the market version — one-click update",
    installedCheck: "Installed ✓",
    updateTo: "Update to v{version}",
    installedPlain: "Installed — available in the tool list",
    installedWithVersion: "Installed v{version} — available in the tool list",
    installedTitle: "Installed plugins",
    newPlugin: "New plugin",
    emptyInstalled:
      "No plugins yet — the app no longer bundles plugins; all are installed from Git registries (the plugin market above). They sync automatically on startup; click “Refresh” or install one from the market.",
    editTitleEnabled: "Edit script (validated before atomic write-back)",
    editTitleDisabled: "Disabled plugins cannot be edited; enable it first",
    toggleDisable: "Disable plugin",
    toggleEnable: "Enable plugin",
    deleteTitle: "Delete plugin {name} (removes the definition only; installed versions are kept)",
    enabled: "Enabled",
    disabled: "Disabled",
    distributionsCount: "{count} distributions",
    downloadedVersions: "{count} versions downloaded",
    notDownloaded: "Not downloaded",
    updatedAt: "Updated {time}",
    luaDirHint: "Lua plugin directory: ~/.envhive/plugins/<name>/plugin.lua (edit files directly)",
    editorEditTitle: "Edit plugin: {name}",
    editorNewTitle: "New Lua plugin",
    pluginNamePlaceholder: "Plugin name (e.g. python)",
    luaEditableTip: "Editable; validated before atomic write-back on save",
    luaHint:
      "Lifecycle hooks: available(ctx) / pre_install(ctx) / post_install(ctx) / env_keys(ctx) / pre_uninstall(ctx); built-in modules http.get (NETWORK_ALLOW allowlist), json, archiver.extract, file (~/.envhive and temp dirs only), versions.parse (SDKMAN-style identifier parsing); plugins may require private modules from a lib/ subdirectory.",
    dryRunNonEmpty:
      "Script is non-empty and can be saved (full dry-run validation awaits the backend plugin_validate hook)",
    dryRunEmpty: "Script is empty",
    dryRun: "Test run (dry-run)",
    saveChanges: "Save changes",
    savePlugin: "Save plugin",
    source: {
      market: "Market",
      local: "Local",
    },
    provider: {
      lua: "Lua script",
    },
  },

  // ---------- Registry (mirror) page ----------
  network: {
    title: "One-click mirror switching",
    hint: "Each row: tool + current source URL + preset dropdown + status. Add custom mirrors anytime; the built-in official source is always kept.",
    custom: "Custom",
    notConfigured: "Not configured",
    conflict: "⚠ Baseline mismatch",
    ackBaseline: "Confirm baseline",
    configFile: "Config file: ",
    customLabel: "Custom source",
    noCustom: "None yet — you can add a custom mirror URL",
    deleteCustom: "Delete {name}",
    addCustomToggle: "＋ Add custom source",
    namePlaceholder: "Name, e.g. my-registry",
    presetPlaceholder: "Select preset…",
    noPreset: "No preset",
    officialSuffix: " (official)",
    customSuffix: " (custom)",
  },

  // ---------- Logs page ----------
  logs: {
    title: "Log viewer",
    exportCopy: "Export / copy",
    file: "File: ",
    level: "Level: ",
    levelAll: "All",
    noLogFiles: "(no log files)",
    selectFile: "Select file",
    fileCount: "{count} files · {dir}",
    emptyHint: "Written automatically to ~/.envhive/logs/envhive.log after startup",
    msgDirUnknown: "Log directory unknown",
    msgDirOpened: "Log directory opened",
    msgDirOpenFailed: "Failed to open the directory (unavailable in preview mode)",
    msgNoContent: "No log content",
    msgCopied: "Logs copied to clipboard",
    msgCopyFailed: "Copy failed (unavailable in preview mode)",
  },

  // ---------- Statistics page ----------
  stats: {
    previewHint: "Unavailable in preview mode; statistics resume once the backend is connected.",
    usageTitle: "Usage statistics (switches in the last 30 days)",
    times: "{count} times",
    summary: "{versions} versions installed · {disk} on disk",
    storageTitle: "Storage usage",
    totalDisk: "Total {disk}",
    colTool: "Tool",
    colVersion: "Version",
    allVersions: "All {count} versions",
    colCount: "Uses",
    colLastUsed: "Last used",
    neverUsed: "Never used",
    daysAgo: "{n} d ago",
    colDisk: "Disk usage",
    colStatus: "Status",
    currentStar: "Current ★",
    notInstalled: "Not installed",
    colOps: "Actions",
  },

  // ---------- About page ----------
  about: {
    tagline: "Multi-language runtimes · environment configuration hub",
    techTitle: "Tech stack",
    repoTitle: "Open-source repositories",
    repoHint: "Click to open the repository / license text. Stars, issues and PRs are welcome — let's build the EnvHive ecosystem together.",
    footer: "{app} · v{version} · built {time}",
    arch: {
      desktop: { layer: "Desktop framework", detail: "Rust backend, ~10MB installer" },
      frontend: { layer: "Frontend", detail: "Vite + Naive UI" },
      backend: { layer: "Backend", detail: "Every tool is driven by a Lua plugin" },
    },
    licenseDisplay: "Mulan PSL v2 · Mulan Permissive Software License, v2",
  },
};

export default enUS;
