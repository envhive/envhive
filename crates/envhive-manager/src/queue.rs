//! 下载队列（roadmap P1·体验）：批量安装多个 工具，串行 worker 执行，各自独立进度。
//! 事件：`queue-updated` 向前端推送队列状态。
//!
//! 并发模型：任意时刻至多一个 worker 运行（`running` 标志互斥）；worker 串行消费
//! 整个队列，每完成一个任务释放 `running` 再取下一个，队列空时退出。
//! 取消：排队中任务直接置 Cancelled；执行中任务通过 `AtomicBool` 取消标志通知
//! 下载循环提前终止（worker 收尾时清理 .part 归档并标记 Cancelled）。

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use envhive_core::error::{EnvHiveError, EnvHiveErrorKind, Result};
use crate::events::{EventSink, ManagerEvent, NullSink};
use crate::manager::EnvHiveManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueTask {
    pub id: u64,
    pub tool: String,
    pub version: String,
    /// v2：发行商 key（Lua 插件发行商维度；无则 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distribution: Option<String>,
    pub status: TaskStatus,
    pub percent: f32,
    pub message: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
}

/// 入队结果：`reused = true` 表示队列中已有同 tool+version+distribution 的
/// 排队/执行中任务，直接复用其 id（前端据此提示，避免重复入队）。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueOutcome {
    pub id: u64,
    pub reused: bool,
}

#[derive(Default)]
struct QueueInner {
    tasks: VecDeque<QueueTask>,
    running: bool,
}

/// 安装任务队列（AppState 持有）
pub struct QueueManager {
    inner: Mutex<QueueInner>,
    next_id: Mutex<u64>,
    /// 执行中任务的取消标志：id -> flag（enqueue 时创建，任务终态后移除）
    cancel_flags: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    /// 事件接收器（桌面=Tauri 事件，CLI/TUI=channel；默认丢弃）
    sink: Arc<dyn EventSink>,
}

impl QueueManager {
    pub fn new() -> Self {
        Self::with_event_sink(Arc::new(NullSink))
    }

    /// 注入事件接收器（桌面端传 Tauri 实现；CLI/TUI 传 channel 实现）
    pub fn with_event_sink(sink: Arc<dyn EventSink>) -> Self {
        QueueManager {
            inner: Mutex::new(QueueInner::default()),
            next_id: Mutex::new(1),
            cancel_flags: Mutex::new(HashMap::new()),
            sink,
        }
    }

    /// 入队；返回任务 id。
    /// 去重：队列中已有同 tool+version+distribution 且处于 Queued/Running 的任务时
    /// 不重复入队，直接返回已有任务 id（reused=true）。终态任务（Done/Failed/Cancelled）
    /// 不参与去重——再次安装（重试）是合法操作。
    pub fn enqueue(&self, tool: String, version: String, distribution: Option<String>) -> EnqueueOutcome {
        let mut inner = self.inner.lock().unwrap();
        if let Some(existing) = inner.tasks.iter().find(|t| {
            t.tool == tool
                && t.version == version
                && t.distribution == distribution
                && matches!(t.status, TaskStatus::Queued | TaskStatus::Running)
        }) {
            return EnqueueOutcome { id: existing.id, reused: true };
        }
        let mut id_guard = self.next_id.lock().unwrap();
        let id = *id_guard;
        *id_guard += 1;
        inner.tasks.push_back(QueueTask {
            id,
            tool,
            version,
            distribution,
            status: TaskStatus::Queued,
            percent: 0.0,
            message: None,
            created_at: now_ts(),
            finished_at: None,
        });
        self.cancel_flags.lock().unwrap().insert(id, Arc::new(AtomicBool::new(false)));
        drop(inner);
        EnqueueOutcome { id, reused: false }
    }

    pub fn snapshot(&self) -> Vec<QueueTask> {
        self.inner.lock().unwrap().tasks.iter().cloned().collect()
    }

    /// 推送当前队列快照（`enqueue` 后立即调用；经事件接收器分发，无 UI 时静默）。
    /// 修复：此前只在 worker 状态切换（running/done）时 emit，任务入队时若无事件，
    /// 前端在「排队中」阶段看不到新任务（表现为下载队列始终只显示一个）。
    pub fn emit_snapshot(&self) {
        self.emit_queue();
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().running
    }

    /// 取消任务：
    /// - Queued：直接置 Cancelled；
    /// - Running：置取消标志，下载循环检测后终止（任务先标记「正在取消」，worker 收尾定终态）；
    /// - 终态（Done/Failed/Cancelled）：返回错误。
    pub fn cancel(&self, id: u64) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let task = inner.tasks.iter_mut().find(|t| t.id == id);
        match task {
            Some(t) if t.status == TaskStatus::Queued => {
                t.status = TaskStatus::Cancelled;
                t.finished_at = Some(now_ts());
                t.message = Some("已取消".into());
                self.cancel_flags.lock().unwrap().remove(&id);
                Ok(())
            }
            Some(t) if t.status == TaskStatus::Running => {
                // 请求取消：置标志，下载循环尽快退出
                if let Some(flag) = self.cancel_flags.lock().unwrap().get(&id) {
                    flag.store(true, Ordering::Relaxed);
                }
                t.message = Some("正在取消…".into());
                Ok(())
            }
            Some(t) => Err(EnvHiveError::new(
                EnvHiveErrorKind::Config,
                format!("任务 {} 处于 {:?}，无法取消", id, t.status),
            )),
            None => Err(EnvHiveError::new(EnvHiveErrorKind::Config, format!("任务 {id} 不存在"))),
        }
    }

    /// 查询任务是否已请求取消（下载循环检测）
    pub fn is_cancel_requested(&self, id: u64) -> bool {
        self.cancel_flags
            .lock()
            .unwrap()
            .get(&id)
            .map(|f| f.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// 全部取消：排队中任务直接置 Cancelled；执行中任务置取消标志（worker 收尾定终态）。
    /// 返回受影响的任务数（含已请求取消的 Running 任务）。
    pub fn cancel_all(&self) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let mut n = 0;
        for t in inner.tasks.iter_mut() {
            match t.status {
                TaskStatus::Queued => {
                    t.status = TaskStatus::Cancelled;
                    t.finished_at = Some(now_ts());
                    t.message = Some("已取消".into());
                    n += 1;
                }
                TaskStatus::Running => {
                    if let Some(flag) = self.cancel_flags.lock().unwrap().get(&t.id) {
                        flag.store(true, Ordering::Relaxed);
                    }
                    t.message = Some("正在取消…".into());
                    n += 1;
                }
                _ => {}
            }
        }
        n
    }

    /// 清空终态任务（Done/Failed/Cancelled），保留排队中/执行中的任务；返回移除数量。
    pub fn clear_finished(&self) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.tasks.len();
        inner.tasks.retain(|t| matches!(t.status, TaskStatus::Queued | TaskStatus::Running));
        before - inner.tasks.len()
    }

    /// 移除任务取消标志（任务进入终态时调用，释放内存）
    fn clear_flag(&self, id: u64) {
        self.cancel_flags.lock().unwrap().remove(&id);
    }

    /// 后台 worker：串行消费队列。至多一个 worker 在跑；
    /// 队列空时重置 `running` 并退出；有新任务由外部再次调用。
    pub async fn run_worker(self: &Arc<Self>, manager: Arc<EnvHiveManager>) {
        loop {
            // 1. 抢执行权（running=false 才可进入）
            {
                let mut inner = self.inner.lock().unwrap();
                if inner.running {
                    return; // 已有 worker 在跑
                }
                inner.running = true;
            }

            // 2+3. 取下一个排队任务，并在同一把锁内立即标记为运行中。
            // 关键修复：原实现先「克隆」任务（步骤2）再在「另一把锁」里置 Running（步骤3），
            // 两者之间存在竞态窗口——若 cancel() 在此窗口内把任务置为 Cancelled，
            // 步骤3 仍会把它改回 Running 并执行，导致「已取消的排队任务仍被安装」。
            // 现在取任务与置 Running 原子完成：cancel() 要么发生在之前（任务已 Cancelled，被跳过），
            // 要么发生在之后（任务已 Running，cancel 走 Running 分支置取消标志，安装循环会提前终止）。
            let task = {
                let mut inner = self.inner.lock().unwrap();
                let pos = inner.tasks.iter().position(|t| t.status == TaskStatus::Queued);
                let Some(pos) = pos else {
                    inner.running = false;
                    return;
                };
                let t = &mut inner.tasks[pos];
                t.status = TaskStatus::Running;
                t.message = Some("开始执行".into());
                t.clone()
            };
            self.emit_queue();

            // 4. 执行安装（携带取消标志）
            let result = manager
                .install_tool(
                    &task.tool,
                    &task.version,
                    task.distribution.as_deref(),
                    self.cancel_flag_arc(task.id),
                )
                .await;

            // 5. 收尾：用户请求过取消 → 定 Cancelled；否则按结果定 Done / Failed
            let cancelled = self.is_cancel_requested(task.id);
            {
                let mut inner = self.inner.lock().unwrap();
                if let Some(t) = inner.tasks.iter_mut().find(|t| t.id == task.id) {
                    match (&result, cancelled) {
                        (_, true) => {
                            t.status = TaskStatus::Cancelled;
                            t.message = Some("已取消".into());
                        }
                        (Ok(r), false) => {
                            t.status = TaskStatus::Done;
                            t.percent = 100.0;
                            t.message = Some(r.message.clone());
                        }
                        (Err(e), false) => {
                            t.status = TaskStatus::Failed;
                            t.message = Some(e.to_string());
                            // 错误详情落日志（用户反馈 log 缺失后续错误信息）
                            tracing::error!("[{}] {} 安装失败: {}", task.tool, task.version, e);
                        }
                    }
                    t.finished_at = Some(now_ts());
                }
            }
            self.clear_flag(task.id);
            self.emit_queue();

            // 6. 释放执行权，循环继续取下一个任务
            self.inner.lock().unwrap().running = false;
        }
    }

    /// 取某任务的取消标志（Arc 克隆；任务不存在则返回 None）
    fn cancel_flag_arc(&self, id: u64) -> Option<Arc<AtomicBool>> {
        self.cancel_flags.lock().unwrap().get(&id).cloned()
    }

    /// 经事件接收器推送队列快照（无 UI 场景为 NullSink，静默丢弃）
    fn emit_queue(&self) {
        self.sink.emit(ManagerEvent::QueueUpdated(self.snapshot()));
    }
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
