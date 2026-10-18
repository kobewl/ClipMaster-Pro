//! macOS 剪贴板采集与写入实现。
//!
//! 支持 Text + Image 两种类型。
//! 防自拷贝循环：writer 写入前记录 fingerprint，采集侧匹配后跳过。
//!
//! 采集顺序：暂停 → 读剪贴板类型 → 隐私类型 → 排除应用 → 来源 URL → 读内容。
//! 前几步都在读取内容、查浏览器 URL（会开 osascript 子进程）、编码 / 落盘图片
//! **之前**返回；读不到类型列表时按隐私优先同样跳过（失败关闭）。
//! 任何一次跳过都会丢弃待消费的自写指纹（见 [`discard_pending_self_writes`]）。

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use clipboard_rs::{
    common::RustImage, Clipboard, ClipboardContent, ClipboardContext, ClipboardHandler,
    ClipboardWatcher, ClipboardWatcherContext, RustImageData,
};
use tokio::sync::mpsc::Sender;

use crate::domain::capture_policy::{CaptureGate, SkipReason};
use crate::domain::error::ClipboardSourceError;
use crate::domain::model::{ContentType, MAX_CONTENT_BYTES};
use crate::domain::normalize::{compute_fingerprint, compute_fingerprint_bytes, strip_html_tags};
use crate::domain::ports::{ClipboardEvent, ClipboardSource, ClipboardWriter};
use crate::infrastructure::appicon::macos::{frontmost_app_source, FrontmostApp};

/// 待消费的 self-write 指纹队列（先进先出，上限 8 个）。
///
/// 用队列而不是单值：一次「写 HTML」会同时放 HTML 和纯文本两份内容进剪贴板，
/// 采集侧先读到哪份不确定，两份指纹都得能对上。
pub type SelfWriteGuard = Arc<Mutex<Vec<String>>>;

const SELF_WRITE_QUEUE_CAP: usize = 8;

pub fn new_self_write_guard() -> SelfWriteGuard {
    Arc::new(Mutex::new(Vec::new()))
}

/// 记下「接下来这次剪贴板变化是我们自己写的」。
/// 超过上限时丢掉最旧的 —— 正常使用远达不到这个量级，防御性兜底而已。
fn mark_self_write(guard: &SelfWriteGuard, fingerprints: &[String]) {
    let mut guard = guard.lock().expect("mutex poisoned");
    for fingerprint in fingerprints {
        guard.push(fingerprint.clone());
    }
    let overflow = guard.len().saturating_sub(SELF_WRITE_QUEUE_CAP);
    guard.drain(..overflow);
}

/// 采集到的内容指纹若在队列里，消费掉并返回 true（说明是自己写回的，跳过）。
fn matches_and_consume_self_write(guard: &SelfWriteGuard, fingerprint: &str) -> bool {
    let mut guard = guard.lock().expect("mutex poisoned");
    if let Some(pos) = guard.iter().position(|fp| fp == fingerprint) {
        guard.remove(pos);
        true
    } else {
        false
    }
}

/// 这次变化被跳过时，清空所有待消费的自写指纹。
///
/// 跳过时不能读内容（隐私闸门的前提），没法只消费匹配的那一枚，只能整队清空。
/// 否则暂停期间从历史粘贴登记的指纹会一直留着，恢复后用户真的复制同样内容时，
/// 那一次会被误当成自写而丢掉。
///
/// 代价：若恰好在「writer 登记指纹」与「它的写入触发变化」之间插进一次被跳过的
/// 外部变化，那次自写会被当成新复制多记一次。窗口只有微秒级，结果也只是触发
/// `insert_or_touch` 的去重更新，不丢数据。
fn discard_pending_self_writes(guard: &SelfWriteGuard) {
    guard.lock().expect("mutex poisoned").clear();
}

/// 读内容之前跳过这次变化的原因。日志里只记它，不记内容和路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureSkip {
    /// 采集闸门：暂停 / 隐私类型 / 排除应用。
    Gate(SkipReason),
    /// 打不开剪贴板上下文。
    ContextUnavailable,
    /// 读不到剪贴板类型列表：没法确认有没有隐私标记，按隐私优先跳过。
    TypesUnreadable,
}

/// 读内容之前的跳过判断（纯函数，便于单测）。
/// 顺序：暂停 → 类型读不到（失败关闭）→ 闸门的隐私类型 / 排除应用。
fn pre_capture_skip<E>(
    gate: &CaptureGate,
    types: &Result<Vec<String>, E>,
    bundle_id: Option<&str>,
    app_name: Option<&str>,
) -> Option<CaptureSkip> {
    if !gate.is_enabled() {
        return Some(CaptureSkip::Gate(SkipReason::Paused));
    }
    match types {
        Err(_) => Some(CaptureSkip::TypesUnreadable),
        Ok(types) => gate
            .skip_reason(types, bundle_id, app_name)
            .map(CaptureSkip::Gate),
    }
}

struct EventHandler {
    sender: Sender<ClipboardEvent>,
    self_write_guard: SelfWriteGuard,
    image_dir: PathBuf,
    /// 与 pipeline / AppRuntime 共享的同一个闸门（采集开关 + 排除名单）。
    gate: CaptureGate,
}

impl ClipboardHandler for EventHandler {
    fn on_clipboard_change(&mut self) {
        // 暂停就是真暂停：连剪贴板上下文都不开，更不查来源、不读内容、不写图片。
        if !self.gate.is_enabled() {
            self.skip_change(CaptureSkip::Gate(SkipReason::Paused));
            return;
        }

        let ctx = match ClipboardContext::new() {
            Ok(ctx) => ctx,
            Err(err) => {
                tracing::warn!(error = %err, "打开 macOS 剪贴板上下文失败");
                self.skip_change(CaptureSkip::ContextUnavailable);
                return;
            }
        };

        // 只读 NSPasteboard 的类型列表（进程内调用，不开子进程、不读内容），
        // 前台应用身份同样是进程内查询。此时前台应用即复制源。
        let types = ctx.available_formats();
        if let Err(err) = &types {
            tracing::debug!(error = %err, "读取剪贴板类型失败，按隐私优先跳过");
        }
        let app = frontmost_app_source();
        let (bundle_id, app_name) = match &app {
            Some(app) => (app.bundle_id.as_deref(), app.name.as_deref()),
            None => (None, None),
        };
        if let Some(skip) = pre_capture_skip(&self.gate, &types, bundle_id, app_name) {
            self.skip_change(skip);
            return;
        }

        // 闸门放行后才查浏览器 URL（会开 osascript 子进程）。
        let source_info = source_info_from(app);

        // 优先文件引用：Finder 里 ⌘C 文件时，剪贴板上会同时出现「文件 URL」和
        // 一段路径样子的文本。先查文件，才能把文件拷贝和普通文本区分开。
        if let Ok(files) = ctx.get_files() {
            if !files.is_empty() {
                self.handle_files(files, source_info);
                return;
            }
        }

        // 其次纯文本。注意浏览器等应用复制富文本时会同时带 text/plain 和 text/html，
        // 这里故意让纯文本赢：只有 HTML、没有纯文本的情况才单独收 HTML 条目。
        if let Ok(text) = ctx.get_text() {
            if !text.is_empty() {
                self.handle_text(text, source_info);
                return;
            }
        }

        // 纯 HTML（没有纯文本兜底）：某些网页编辑器只往剪贴板放 text/html。
        if let Ok(html) = ctx.get_html() {
            if !html.is_empty() {
                self.handle_html(html, source_info);
                return;
            }
        }

        // 最后尝试图片
        self.handle_image(&ctx, source_info);
    }
}

struct SourceInfo {
    app_name: Option<String>,
    source_url: Option<String>,
    bundle_id: Option<String>,
    app_path: Option<String>,
}

fn source_info_from(app: Option<FrontmostApp>) -> SourceInfo {
    // 前台应用身份走 NSWorkspace 进程内查询：零子进程、零「自动化」权限依赖。
    // 旧 osascript 路径（System Events）实测每次复制要付 40ms+ 的子进程底价，
    // 且授权被拒后来源永远为空。失败时各字段为 None，按「无来源」记录。
    // bundle id 与 `.app` 路径用来事后取真实图标，这里不渲染、不碰磁盘。
    let (app_name, bundle_id, app_path) = match app {
        Some(app) => (app.name, app.bundle_id, app.app_path),
        None => (None, None, None),
    };
    // 只有前台是受支持的浏览器时才会真正 spawn osascript 去查标签页 URL ——
    // 每个浏览器的 AppleScript 字典没有进程内替代，这条子进程保留。
    let source_url = get_browser_url(bundle_id.as_deref(), app_name.as_deref());
    SourceInfo {
        app_name,
        source_url,
        bundle_id,
        app_path,
    }
}

/// 取当前标签页 URL 的两种 AppleScript 写法。
#[derive(Clone, Copy)]
enum TabSyntax {
    /// Safari 系：`current tab`。
    Safari,
    /// Chromium 系：`active tab`。
    Chromium,
}

/// 受支持的浏览器：(bundle id, AppleScript 应用名, 语法)。
///
/// 每个浏览器都用**自己的**应用名：Chromium / Chrome Canary / Edge Beta 等都是独立
/// 应用，写成 `tell application "Google Chrome"` 会去问（甚至拉起）另一个应用。
const BROWSERS: &[(&str, &str, TabSyntax)] = &[
    ("com.apple.Safari", "Safari", TabSyntax::Safari),
    (
        "com.apple.SafariTechnologyPreview",
        "Safari Technology Preview",
        TabSyntax::Safari,
    ),
    ("com.google.Chrome", "Google Chrome", TabSyntax::Chromium),
    (
        "com.google.Chrome.beta",
        "Google Chrome Beta",
        TabSyntax::Chromium,
    ),
    (
        "com.google.Chrome.dev",
        "Google Chrome Dev",
        TabSyntax::Chromium,
    ),
    (
        "com.google.Chrome.canary",
        "Google Chrome Canary",
        TabSyntax::Chromium,
    ),
    ("org.chromium.Chromium", "Chromium", TabSyntax::Chromium),
    (
        "com.microsoft.edgemac",
        "Microsoft Edge",
        TabSyntax::Chromium,
    ),
    (
        "com.microsoft.edgemac.Beta",
        "Microsoft Edge Beta",
        TabSyntax::Chromium,
    ),
    (
        "com.microsoft.edgemac.Dev",
        "Microsoft Edge Dev",
        TabSyntax::Chromium,
    ),
    (
        "com.microsoft.edgemac.Canary",
        "Microsoft Edge Canary",
        TabSyntax::Chromium,
    ),
    ("company.thebrowser.Browser", "Arc", TabSyntax::Chromium),
    ("com.brave.Browser", "Brave Browser", TabSyntax::Chromium),
];

/// osascript 查 URL 的最长等待。超时直接 kill，采集不能卡在这里。
const BROWSER_URL_TIMEOUT: Duration = Duration::from_millis(300);

/// 各浏览器取「当前标签页 URL」的 AppleScript；None = 不是受支持的浏览器。
///
/// 先按 bundle id 匹配（中文系统下 Safari 的显示名可能是「Safari浏览器」），
/// 再按应用名精确匹配。脚本里的名字只来自 [`BROWSERS`] 常量表，不拼接外部输入；
/// `if application "X" is running` 保证不会把一个没在运行的浏览器拉起来。
///
/// 抽成纯函数是为了能单测映射关系（哪些应用会触发子进程、哪些不会）。
fn browser_script_for(bundle_id: Option<&str>, app_name: Option<&str>) -> Option<String> {
    let by_bundle = bundle_id.and_then(|id| {
        BROWSERS
            .iter()
            .find(|(bundle, _, _)| bundle.eq_ignore_ascii_case(id))
    });
    let (_, name, syntax) = by_bundle
        .or_else(|| app_name.and_then(|app| BROWSERS.iter().find(|(_, name, _)| *name == app)))?;
    let tab = match syntax {
        TabSyntax::Safari => "current tab",
        TabSyntax::Chromium => "active tab",
    };
    Some(format!(
        r#"if application "{name}" is running then tell application "{name}" to get URL of {tab} of front window"#
    ))
}

/// 跑一个子进程，最多等 `timeout`；超时就 kill 并返回 None。
fn output_with_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn get_browser_url(bundle_id: Option<&str>, app_name: Option<&str>) -> Option<String> {
    let script = browser_script_for(bundle_id, app_name)?;
    let output = output_with_timeout(
        Command::new("osascript").args(["-e", &script]),
        BROWSER_URL_TIMEOUT,
    )?;
    if !output.status.success() {
        return None;
    }
    // 浏览器没在运行时 `if` 不成立，输出为空。
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() || url == "missing value" {
        None
    } else {
        Some(url)
    }
}

impl EventHandler {
    /// 跳过这次变化：只记原因（不记内容、应用路径），并丢弃待消费的自写指纹。
    fn skip_change(&self, skip: CaptureSkip) {
        tracing::debug!(?skip, "跳过本次剪贴板变化");
        discard_pending_self_writes(&self.self_write_guard);
    }

    fn handle_files(&self, files: Vec<String>, source: SourceInfo) {
        let files: Vec<String> = files
            .into_iter()
            .filter(|path| !path.trim().is_empty())
            .collect();
        if files.is_empty() {
            return;
        }

        // 多个路径按剪贴板给出的顺序用 \n 连接后一起参与指纹 ——
        // 同一批文件去重；顺序不同算不同内容（顺序本身是有意义的信息）。
        let joined = files.join("\n");
        let fingerprint = compute_fingerprint("files", &joined);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }

        self.send_event(ClipboardEvent {
            content_type: ContentType::Files,
            content_text: joined,
            source_app: source.app_name,
            source_url: source.source_url,
            source_bundle_id: source.bundle_id,
            source_app_path: source.app_path,
        });
    }

    fn handle_text(&self, text: String, source: SourceInfo) {
        // 超大文本反正会被 HistoryService 拒绝，在这里就跳过，省掉对几 MB 文本的
        // 规范化 + 哈希和一次跨线程搬运。日志只记大小，不记内容。
        // 跳过时不消费自写指纹：唯一可能残留的是合并复制写入的超大文本指纹，
        // 它只会匹配同一段反正也会被跳过的文本，无害。
        if text.len() > MAX_CONTENT_BYTES {
            tracing::warn!(
                size_bytes = text.len(),
                limit = MAX_CONTENT_BYTES,
                "剪贴板文本超过大小上限，watcher 直接跳过"
            );
            return;
        }
        let fingerprint = compute_fingerprint("text", &text);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }
        self.send_event(ClipboardEvent {
            content_type: ContentType::Text,
            content_text: text,
            source_app: source.app_name,
            source_url: source.source_url,
            source_bundle_id: source.bundle_id,
            source_app_path: source.app_path,
        });
    }

    fn handle_html(&self, html: String, source: SourceInfo) {
        // 同 handle_text：HistoryService 对 HTML 用同一个上限，先判大小再算指纹。
        // 跳过时同样不消费自写指纹（残留的只可能是同一段超大内容的指纹，无害）。
        if html.len() > MAX_CONTENT_BYTES {
            tracing::warn!(
                size_bytes = html.len(),
                limit = MAX_CONTENT_BYTES,
                "剪贴板 HTML 超过大小上限，watcher 直接跳过"
            );
            return;
        }
        let fingerprint = compute_fingerprint("html", &html);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }
        self.send_event(ClipboardEvent {
            content_type: ContentType::Html,
            content_text: html,
            source_app: source.app_name,
            source_url: source.source_url,
            source_bundle_id: source.bundle_id,
            source_app_path: source.app_path,
        });
    }

    fn handle_image(&self, ctx: &ClipboardContext, source: SourceInfo) {
        let image = match ctx.get_image() {
            Ok(img) => img,
            Err(_) => return,
        };

        if image.is_empty() {
            return;
        }

        let png_buf = match image.to_png() {
            Ok(buf) => buf,
            Err(err) => {
                tracing::debug!(error = %err, "图片转 PNG 失败，跳过");
                return;
            }
        };

        let png_bytes = png_buf.get_bytes();
        if png_bytes.is_empty() {
            return;
        }

        let fingerprint = compute_fingerprint_bytes("image", png_bytes);
        if matches_and_consume_self_write(&self.self_write_guard, &fingerprint) {
            return;
        }

        // 用 fingerprint 前 32 字符做文件名，同内容不重复写文件
        let file_name = format!("{}.png", &fingerprint[..32.min(fingerprint.len())]);
        let file_path = self.image_dir.join(&file_name);

        // 编码途中被暂停：不落盘，不留孤儿图片。
        if !self.gate.is_enabled() {
            self.skip_change(CaptureSkip::Gate(SkipReason::Paused));
            return;
        }

        if !file_path.exists() {
            if let Err(err) = std::fs::write(&file_path, png_bytes) {
                tracing::warn!(error = %err, "保存截图到文件失败");
                return;
            }
        }

        self.send_event(ClipboardEvent {
            content_type: ContentType::Image,
            content_text: file_path.to_string_lossy().to_string(),
            source_app: source.app_name,
            source_url: source.source_url,
            source_bundle_id: source.bundle_id,
            source_app_path: source.app_path,
        });
    }

    /// 把事件交给采集管线，队列满时阻塞当前线程（背压）。
    ///
    /// 调用线程是 watcher 自己的 OS 线程（`start` 里 `thread::Builder` 起的，
    /// clipboard-rs 在它上面同步回调），不在 tokio 上下文里，所以可以用
    /// `blocking_send`。队列满了就阻塞 watcher：阻塞期间不轮询，中间的剪贴板
    /// 变化会被 changeCount 轮询合并成最新一次。不用 `try_send`：满了会丢掉最新
    /// 那条，而那正是用户最想要的。以前每个事件 spawn 一个任务，等于绕过了有界队列。
    fn send_event(&self, event: ClipboardEvent) {
        if let Err(err) = self.sender.blocking_send(event) {
            tracing::warn!(error = %err, "剪贴板事件发送失败（采集管线已关闭）");
        }
    }
}

// ---------------------------------------------------------------------------
// ClipboardSource
// ---------------------------------------------------------------------------

pub struct MacOsClipboardSource {
    self_write_guard: SelfWriteGuard,
    image_dir: PathBuf,
    gate: CaptureGate,
    running: Arc<AtomicBool>,
    shutdown: Option<clipboard_rs::WatcherShutdown>,
}

impl MacOsClipboardSource {
    pub fn new(self_write_guard: SelfWriteGuard, image_dir: PathBuf, gate: CaptureGate) -> Self {
        Self {
            self_write_guard,
            image_dir,
            gate,
            running: Arc::new(AtomicBool::new(false)),
            shutdown: None,
        }
    }
}

impl ClipboardSource for MacOsClipboardSource {
    fn start(&mut self, sender: Sender<ClipboardEvent>) -> Result<(), ClipboardSourceError> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let self_write_guard = self.self_write_guard.clone();
        let image_dir = self.image_dir.clone();
        let running = self.running.clone();

        let mut watcher = ClipboardWatcherContext::new()
            .map_err(|e| ClipboardSourceError::ReadFailed(e.to_string()))?;

        let handler = EventHandler {
            sender,
            self_write_guard,
            image_dir,
            gate: self.gate.clone(),
        };
        watcher.add_handler(handler);
        let shutdown = watcher.get_shutdown_channel();
        self.shutdown = Some(shutdown);
        running.store(true, Ordering::SeqCst);

        thread::Builder::new()
            .name("clipmaster-clipboard-watcher".into())
            .spawn(move || {
                watcher.start_watch();
                running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| ClipboardSourceError::ReadFailed(e.to_string()))?;

        Ok(())
    }

    fn stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.stop();
        }
        self.running.store(false, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// ClipboardWriter
// ---------------------------------------------------------------------------

/// 把 HTML 组装成剪贴板的 representations：HTML 必带，纯文本兜底仅在
/// 剥离后仍有可读内容时附带。
///
/// 必须用 `ctx.set(contents)` **一次**写入。clipboard-rs 在 macOS 上的
/// `set_html`/`set_text` 是各自「先 clearContents() 再写」，分两次调用
/// 第二次会把第一次的 representation 整个清掉 —— 粘贴退化成纯文本；
/// 单次 `set` 才是同一个 NSPasteboardItem 挂多个 representation。
fn clipboard_contents_for_html(html: &str, plain: &str) -> Vec<ClipboardContent> {
    let mut contents = vec![ClipboardContent::Html(html.to_string())];
    if !plain.trim().is_empty() {
        contents.push(ClipboardContent::Text(plain.to_string()));
    }
    contents
}

/// HTML 写入的自写守护指纹：与 [`clipboard_contents_for_html`] 产出的
/// representation 一一对应 —— 多一份内容就多一个指纹，少一份就少一个。
fn html_write_fingerprints(html: &str, plain: &str) -> Vec<String> {
    let mut fingerprints = vec![compute_fingerprint("html", html)];
    if !plain.trim().is_empty() {
        fingerprints.push(compute_fingerprint("text", plain));
    }
    fingerprints
}

pub struct MacOsClipboardWriter {
    self_write_guard: SelfWriteGuard,
}

impl MacOsClipboardWriter {
    pub fn new(self_write_guard: SelfWriteGuard) -> Self {
        Self { self_write_guard }
    }
}

impl ClipboardWriter for MacOsClipboardWriter {
    fn write_text(&self, content: &str) -> Result<(), ClipboardSourceError> {
        let fingerprint = compute_fingerprint("text", content);
        mark_self_write(&self.self_write_guard, &[fingerprint]);
        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set_text(content.to_string())
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        Ok(())
    }

    fn write_image(&self, image_path: &str) -> Result<(), ClipboardSourceError> {
        let png_bytes = std::fs::read(image_path)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("读取图片文件失败: {e}")))?;

        let fingerprint = compute_fingerprint_bytes("image", &png_bytes);
        mark_self_write(&self.self_write_guard, &[fingerprint]);

        // RustImageData::from_bytes 内部用 image crate 解码 PNG → DynamicImage
        let image_data = RustImageData::from_bytes(&png_bytes)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("PNG 解码失败: {e}")))?;

        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set_image(image_data)
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        Ok(())
    }

    fn write_html(&self, html: &str) -> Result<(), ClipboardSourceError> {
        if html.trim().is_empty() {
            return Err(ClipboardSourceError::WriteFailed(
                "HTML 内容为空".to_string(),
            ));
        }

        // 同时带一份纯文本兜底：目标应用若只认 text/plain（终端、多数输入框），
        // 粘出来的也是可读内容，而不是什么都没有。两个 representation 必须
        // 单次 set 写入（见 clipboard_contents_for_html 的注释）。
        let plain = strip_html_tags(html);
        let contents = clipboard_contents_for_html(html, &plain);
        mark_self_write(
            &self.self_write_guard,
            &html_write_fingerprints(html, &plain),
        );

        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set(contents)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("写入剪贴板失败: {e}")))
    }

    fn write_files(&self, paths: &[String]) -> Result<(), ClipboardSourceError> {
        let paths: Vec<String> = paths
            .iter()
            .filter(|path| !path.trim().is_empty())
            .cloned()
            .collect();
        if paths.is_empty() {
            return Err(ClipboardSourceError::WriteFailed(
                "文件列表为空".to_string(),
            ));
        }

        // 指纹口径必须与采集侧一致：同样按顺序用 \n 连接后计算。
        let fingerprint = compute_fingerprint("files", &paths.join("\n"));
        mark_self_write(&self.self_write_guard, &[fingerprint]);

        let ctx = ClipboardContext::new()
            .map_err(|e| ClipboardSourceError::WriteFailed(e.to_string()))?;
        ctx.set_files(paths)
            .map_err(|e| ClipboardSourceError::WriteFailed(format!("写入文件引用失败: {e}")))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_write_guard_is_consumed_after_first_match() {
        let guard = new_self_write_guard();
        mark_self_write(&guard, &["fp-1".to_string()]);
        assert!(matches_and_consume_self_write(&guard, "fp-1"));
        assert!(guard.lock().unwrap().is_empty());
        assert!(!matches_and_consume_self_write(&guard, "fp-1"));
    }

    #[test]
    fn self_write_guard_does_not_match_different_fingerprint() {
        let guard = new_self_write_guard();
        mark_self_write(&guard, &["fp-1".to_string()]);
        assert!(!matches_and_consume_self_write(&guard, "fp-2"));
        assert_eq!(guard.lock().unwrap().as_slice(), ["fp-1".to_string()]);
    }

    #[test]
    fn html_write_marks_both_html_and_plain_text_fingerprints() {
        // 写 HTML 时同时放两份内容进剪贴板；采集侧无论先读到哪份都得能对上。
        let guard = new_self_write_guard();
        mark_self_write(&guard, &["fp-html".to_string(), "fp-plain".to_string()]);

        // 采集侧先读到纯文本 → 消费纯文本指纹；
        // HTML 指纹仍在队列里 —— 若目标应用清掉了 text，采集侧后来读到
        // text/html 时依然能对上，不会把回写当成用户的新复制。
        assert!(matches_and_consume_self_write(&guard, "fp-plain"));
        assert!(matches_and_consume_self_write(&guard, "fp-html"));
        assert!(guard.lock().unwrap().is_empty());
        assert!(!matches_and_consume_self_write(&guard, "fp-plain"));
    }

    #[test]
    fn self_write_queue_caps_at_limit_dropping_oldest() {
        let guard = new_self_write_guard();
        let fingerprints: Vec<String> = (0..12).map(|i| format!("fp-{i}")).collect();
        mark_self_write(&guard, &fingerprints);

        let queued = guard.lock().unwrap().clone();
        assert_eq!(queued.len(), 8);
        assert_eq!(queued[0], "fp-4", "超出上限时丢掉最旧的");
        assert_eq!(queued[7], "fp-11");
    }

    #[test]
    fn html_write_contents_carry_both_representations_for_one_set() {
        // Bugbot P1 回归：富文本写回必须把 Html 和 Text 组装成一个向量、
        // 单次 ctx.set 写入。clipboard-rs 的 set_html/set_text 各自先
        // clearContents() 再写，分两次调用第二次会把第一次清掉，
        // 粘贴就退化成纯文本了。
        let contents = clipboard_contents_for_html("<b>你好</b> 世界", "你好 世界");

        assert_eq!(
            contents.len(),
            2,
            "有可读纯文本时必须是 Html + Text 两个 representation"
        );
        match &contents[0] {
            ClipboardContent::Html(data) => assert_eq!(data.as_str(), "<b>你好</b> 世界"),
            _ => panic!("第一项应是 HTML representation"),
        }
        match &contents[1] {
            ClipboardContent::Text(data) => assert_eq!(data.as_str(), "你好 世界"),
            _ => panic!("第二项应是纯文本 representation"),
        }
    }

    #[test]
    fn html_write_skips_plain_text_fallback_when_nothing_readable() {
        // HTML 剥离后没有可读文本（纯图片、纯样式标签）时不能附带空白
        // text representation —— 有些应用优先拿 text，粘出来就是空的。
        for html in ["<img src='x.png'/>", "<div><br/></div>", "<span>  </span>"] {
            let plain = strip_html_tags(html);
            assert!(plain.trim().is_empty(), "用例 {html} 的剥离结果应为空白");
            let contents = clipboard_contents_for_html(html, &plain);
            assert_eq!(contents.len(), 1, "html = {html} 不应附带空白纯文本");
            assert!(matches!(&contents[0], ClipboardContent::Html(_)));
        }
    }

    #[test]
    fn html_write_fingerprints_match_written_representations() {
        // 自写守护登记的指纹必须与实际写进剪贴板的 representation 一一对应：
        // 空纯文本分支只写 HTML，就只登记 HTML 指纹；否则守护要么脏指纹堆积，
        // 要么在「HTML 有、纯文本无」的真实内容上漏拦。
        let html = "<b>你好</b> 世界";
        let plain = strip_html_tags(html);
        let contents = clipboard_contents_for_html(html, &plain);
        let fingerprints = html_write_fingerprints(html, &plain);

        assert_eq!(fingerprints.len(), contents.len());
        for (content, fingerprint) in contents.iter().zip(&fingerprints) {
            let actual = match content {
                ClipboardContent::Html(data) => compute_fingerprint("html", data.as_str()),
                ClipboardContent::Text(data) => compute_fingerprint("text", data.as_str()),
                _ => panic!("只应有 Html/Text 两种 representation"),
            };
            assert_eq!(&actual, fingerprint, "指纹必须对应实际写入的内容");
        }

        // 纯文本为空白时：一份 representation、一个 HTML 指纹。
        assert_eq!(clipboard_contents_for_html(html, "   ").len(), 1);
        assert_eq!(html_write_fingerprints(html, "   ").len(), 1);
    }

    #[test]
    fn browser_scripts_use_each_browsers_own_name() {
        let script = |bundle: Option<&str>, name: Option<&str>| {
            browser_script_for(bundle, name).unwrap_or_else(|| panic!("{bundle:?} {name:?}"))
        };

        let chromium = script(None, Some("Chromium"));
        assert!(chromium.contains(r#"tell application "Chromium""#));
        assert!(
            !chromium.contains("Google Chrome"),
            "Chromium 不能去问 Google Chrome"
        );
        assert!(script(None, Some("Google Chrome Canary"))
            .contains(r#"tell application "Google Chrome Canary""#));
        assert!(script(None, Some("Microsoft Edge Dev"))
            .contains(r#"tell application "Microsoft Edge Dev""#));
        assert!(script(None, Some("Microsoft Edge Beta"))
            .contains(r#"tell application "Microsoft Edge Beta""#));
        assert!(script(None, Some("Safari Technology Preview"))
            .contains(r#"tell application "Safari Technology Preview""#));
        assert!(script(None, Some("Safari")).contains("current tab"));
        assert!(script(None, Some("Arc")).contains("active tab"));

        // bundle id 优先：中文系统下 Safari 的显示名可能是「Safari浏览器」
        let safari = script(Some("com.apple.Safari"), Some("Safari浏览器"));
        assert!(safari.contains(r#"tell application "Safari" "#));
        assert!(script(Some("com.google.Chrome.canary"), None)
            .contains(r#"tell application "Google Chrome Canary""#));
        assert!(script(Some("COM.BRAVE.BROWSER"), Some("Brave"))
            .contains(r#"tell application "Brave Browser""#));

        // 每条脚本都只在应用已运行时才 tell，不会拉起没在跑的浏览器
        for (bundle, name, _) in BROWSERS {
            let by_bundle = script(Some(bundle), None);
            assert!(
                by_bundle.starts_with(&format!(r#"if application "{name}" is running then"#)),
                "{by_bundle}"
            );
            assert_eq!(by_bundle, script(None, Some(name)));
        }
    }

    #[test]
    fn browser_scripts_only_spawn_osascript_for_supported_browsers() {
        // 非浏览器必须返回 None —— 每次复制都会路过这里，多 spawn 一个子进程都是浪费
        assert_eq!(browser_script_for(None, Some("Finder")), None);
        assert_eq!(
            browser_script_for(Some("com.apple.finder"), Some("Finder")),
            None
        );
        assert_eq!(browser_script_for(None, Some("IntelliJ IDEA")), None);
        assert_eq!(browser_script_for(None, Some("iTerm2")), None);
        // 别让「包含」关系误命中：ChromeHelper 不是浏览器
        assert_eq!(browser_script_for(None, Some("ChromeHelper")), None);
        assert_eq!(
            browser_script_for(Some("com.google.Chrome.helper"), None),
            None
        );
        assert_eq!(browser_script_for(None, None), None);
    }

    #[test]
    fn output_with_timeout_kills_slow_children() {
        let started = Instant::now();
        let output =
            output_with_timeout(Command::new("sleep").arg("5"), Duration::from_millis(200));
        assert!(output.is_none(), "超时必须返回 None");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "超时后必须立刻 kill，不能等子进程自己结束"
        );
    }

    #[test]
    fn output_with_timeout_returns_fast_output() {
        let output = output_with_timeout(Command::new("echo").arg("hi"), Duration::from_secs(2))
            .expect("echo 应在超时前结束");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "hi\n");
    }

    #[test]
    fn concealed_pasteboard_is_skipped_through_the_gate() {
        let gate = CaptureGate::new(true, Vec::new());
        let types = vec![
            "org.nspasteboard.ConcealedType".to_string(),
            "public.utf8-plain-text".to_string(),
        ];
        assert_eq!(
            gate.skip_reason(&types, Some("com.example.vault"), Some("Vault")),
            Some(crate::domain::capture_policy::SkipReason::PrivacyMarked)
        );
        assert_eq!(
            gate.skip_reason(
                &["public.utf8-plain-text".to_string()],
                Some("com.apple.Safari"),
                None
            ),
            None
        );
        gate.set_enabled(false);
        assert_eq!(
            gate.skip_reason(&[], None, None),
            Some(crate::domain::capture_policy::SkipReason::Paused)
        );
    }

    fn ok_types(list: &[&str]) -> Result<Vec<String>, &'static str> {
        Ok(list.iter().map(|t| t.to_string()).collect())
    }

    #[test]
    fn pre_capture_skip_fails_closed_when_types_unreadable() {
        let gate = CaptureGate::new(true, vec!["com.example.vault".to_string()]);
        let unreadable: Result<Vec<String>, &str> = Err("boom");

        assert_eq!(
            pre_capture_skip(&gate, &unreadable, Some("com.apple.Safari"), None),
            Some(CaptureSkip::TypesUnreadable),
            "读不到类型时不能当成「没有隐私标记」放行"
        );
        assert_eq!(
            pre_capture_skip(
                &gate,
                &ok_types(&["public.utf8-plain-text"]),
                Some("com.apple.Safari"),
                Some("Safari")
            ),
            None
        );
        assert_eq!(
            pre_capture_skip(
                &gate,
                &ok_types(&["org.nspasteboard.ConcealedType", "public.utf8-plain-text"]),
                Some("com.apple.Safari"),
                None
            ),
            Some(CaptureSkip::Gate(SkipReason::PrivacyMarked))
        );
        assert_eq!(
            pre_capture_skip(
                &gate,
                &ok_types(&["public.utf8-plain-text"]),
                Some("com.example.vault"),
                None
            ),
            Some(CaptureSkip::Gate(SkipReason::ExcludedApp))
        );

        gate.set_enabled(false);
        assert_eq!(
            pre_capture_skip(&gate, &unreadable, None, None),
            Some(CaptureSkip::Gate(SkipReason::Paused)),
            "暂停优先于类型读取失败"
        );
    }

    #[test]
    fn skipped_change_discards_pending_self_writes() {
        // 暂停期间从历史粘贴：writer 登记了指纹，但 watcher 因暂停跳过了这次变化。
        let guard = new_self_write_guard();
        mark_self_write(&guard, &["fp-paste".to_string()]);
        let gate = CaptureGate::new(false, Vec::new());

        let skip = pre_capture_skip(&gate, &ok_types(&["public.utf8-plain-text"]), None, None);
        assert_eq!(skip, Some(CaptureSkip::Gate(SkipReason::Paused)));
        discard_pending_self_writes(&guard);

        assert!(guard.lock().unwrap().is_empty());
        assert!(
            !matches_and_consume_self_write(&guard, "fp-paste"),
            "恢复后真实复制同样内容必须被记录，不能被残留指纹吞掉"
        );
    }

    #[test]
    fn skipped_excluded_app_change_discards_pending_self_writes() {
        // 往排除名单里的应用粘贴：同样被跳过，同样不能留下残留指纹。
        let guard = new_self_write_guard();
        mark_self_write(&guard, &["fp-html".to_string(), "fp-plain".to_string()]);
        let gate = CaptureGate::new(true, vec!["com.example.vault".to_string()]);

        let skip = pre_capture_skip(
            &gate,
            &ok_types(&["public.utf8-plain-text"]),
            Some("com.example.vault"),
            Some("Vault"),
        );
        assert_eq!(skip, Some(CaptureSkip::Gate(SkipReason::ExcludedApp)));
        discard_pending_self_writes(&guard);

        assert!(guard.lock().unwrap().is_empty());
        assert!(!matches_and_consume_self_write(&guard, "fp-plain"));
        assert!(!matches_and_consume_self_write(&guard, "fp-html"));
    }

    fn test_handler(sender: Sender<ClipboardEvent>) -> EventHandler {
        EventHandler {
            sender,
            self_write_guard: new_self_write_guard(),
            image_dir: std::env::temp_dir(),
            gate: CaptureGate::new(true, Vec::new()),
        }
    }

    fn empty_source() -> SourceInfo {
        SourceInfo {
            app_name: None,
            source_url: None,
            bundle_id: None,
            app_path: None,
        }
    }

    #[test]
    fn oversized_text_and_html_are_dropped_before_fingerprinting() {
        // 同步 #[test]、不开 runtime：与 watcher 线程的真实环境一致。
        let (sender, mut rx) = tokio::sync::mpsc::channel(4);
        let handler = test_handler(sender);
        let oversized = "a".repeat(MAX_CONTENT_BYTES + 1);

        handler.handle_text(oversized.clone(), empty_source());
        assert!(rx.try_recv().is_err(), "超大文本不应进入采集管线");

        handler.handle_html(oversized, empty_source());
        assert!(rx.try_recv().is_err(), "超大 HTML 不应进入采集管线");
    }

    #[test]
    fn normal_text_is_forwarded_from_a_plain_thread() {
        // 普通线程（无 tokio 上下文）上 blocking_send 可用，事件同步入队。
        let (sender, mut rx) = tokio::sync::mpsc::channel(4);
        let handler = test_handler(sender);

        handler.handle_text("hello".to_string(), empty_source());
        let event = rx.try_recv().expect("普通文本应同步进入队列");
        assert_eq!(event.content_type, ContentType::Text);
        assert_eq!(event.content_text, "hello");
    }

    #[test]
    fn send_event_applies_backpressure_when_queue_is_full() {
        let (sender, mut rx) = tokio::sync::mpsc::channel(1);
        let event = |text: &str| ClipboardEvent {
            content_type: ContentType::Text,
            content_text: text.to_string(),
            source_app: None,
            source_url: None,
            source_bundle_id: None,
            source_app_path: None,
        };
        sender.try_send(event("first")).expect("队列有一个空位");

        let handler = test_handler(sender);
        let second = event("second");
        let handle = std::thread::spawn(move || handler.send_event(second));

        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !handle.is_finished(),
            "队列满时 send_event 必须阻塞，而不是另起任务绕过背压"
        );

        assert_eq!(rx.blocking_recv().expect("第一条").content_text, "first");
        assert_eq!(rx.blocking_recv().expect("第二条").content_text, "second");
        handle.join().expect("腾出空位后 send_event 应返回");
    }
}
