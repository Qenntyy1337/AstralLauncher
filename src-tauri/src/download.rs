use futures_util::StreamExt;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::Emitter;

use crate::http::client;
use crate::state::JobState;
use crate::util::ensure_dir;

#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressPayload {
    pub job_id: String,
    pub label: String,
    pub phase: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub remaining_bytes: u64,
    pub files_done: u32,
    pub files_total: u32,
    pub current_file: String,
    pub speed_bps: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskFinished {
    pub job_id: String,
    pub kind: String,
    pub ok: bool,
    pub message: String,
}

pub struct Tracker {
    total: u64,
    downloaded: AtomicU64,
    files_done: AtomicU32,
    files_total: u32,
    last_emit: Mutex<(Instant, u64)>,
}

impl Tracker {
    pub fn new(total: u64, files_total: u32) -> Self {
        Self {
            total,
            downloaded: AtomicU64::new(0),
            files_done: AtomicU32::new(0),
            files_total,
            last_emit: Mutex::new((Instant::now(), 0)),
        }
    }

    pub fn add_downloaded(&self, n: u64) {
        self.downloaded.fetch_add(n, Ordering::Relaxed);
    }

    pub fn file_done(&self) {
        self.files_done.fetch_add(1, Ordering::Relaxed);
    }

    pub fn emit(
        &self,
        app: &tauri::AppHandle,
        job_id: &str,
        label: &str,
        phase: &str,
        current_file: &str,
        force: bool,
    ) {
        let downloaded = self.downloaded.load(Ordering::Relaxed);
        let now = Instant::now();
        let mut last = self.last_emit.lock().unwrap();
        let elapsed = now.duration_since(last.0).as_millis() as u64;
        if !force && elapsed < 150 {
            return;
        }
        let speed = if elapsed > 0 {
            downloaded.saturating_sub(last.1) * 1000 / elapsed
        } else {
            0
        };
        *last = (now, downloaded);
        let payload = ProgressPayload {
            job_id: job_id.to_string(),
            label: label.to_string(),
            phase: phase.to_string(),
            total_bytes: self.total,
            downloaded_bytes: downloaded.min(self.total),
            remaining_bytes: self.total.saturating_sub(downloaded),
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            current_file: current_file.to_string(),
            speed_bps: speed,
        };
        let _ = app.emit("download-progress", &payload);
    }
}

async fn probe_size(item: &DownloadItem) -> u64 {
    if let Some(s) = item.size {
        return s;
    }
    if let Ok(resp) = client().head(&item.url).send().await {
        if let Some(len) = resp.headers().get(reqwest::header::CONTENT_LENGTH) {
            if let Ok(v) = len.to_str().unwrap_or("").parse::<u64>() {
                return v;
            }
        }
    }
    0
}

async fn probe_sizes(items: &[DownloadItem]) -> Vec<u64> {
    let mut out = Vec::with_capacity(items.len());
    let mut start = 0;
    while start < items.len() {
        let end = (start + 24).min(items.len());
        let results =
            futures_util::future::join_all(items[start..end].iter().map(probe_size)).await;
        out.extend(results);
        start = end;
    }
    out
}

fn file_valid(item: &DownloadItem) -> bool {
    if !item.dest.exists() {
        return false;
    }
    match &item.sha1 {
        Some(h) => crate::util::sha1_file(&item.dest)
            .map(|s| s == *h)
            .unwrap_or(false),
        None => match item.size {
            Some(s) => std::fs::metadata(&item.dest)
                .map(|m| m.len() == s)
                .unwrap_or(false),
            None => std::fs::metadata(&item.dest)
                .map(|m| m.len() > 0)
                .unwrap_or(false),
        },
    }
}

async fn download_one(
    app: &tauri::AppHandle,
    job: &Arc<JobState>,
    tracker: &Arc<Tracker>,
    job_id: &str,
    label: &str,
    item: &DownloadItem,
) -> Result<(), String> {
    let name = item
        .dest
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    if job.cancelled() {
        return Err("cancelled".to_string());
    }
    ensure_dir(item.dest.parent().unwrap_or(&PathBuf::from(".")))?;
    if file_valid(item) {
        let existing = item.size.unwrap_or_else(|| {
            std::fs::metadata(&item.dest).map(|m| m.len()).unwrap_or(0)
        });
        tracker.add_downloaded(existing);
        tracker.file_done();
        tracker.emit(app, job_id, label, "download", &name, false);
        return Ok(());
    }
    let part_path = item.dest.with_extension("part");
    let resume_from: u64 = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
    let mut last_err = String::new();
    for attempt in 0u64..3 {
        if job.cancelled() {
            return Err("cancelled".to_string());
        }
        let mut req = client().get(&item.url);
        if resume_from > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={}-", resume_from));
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = e.to_string();
                tokio::time::sleep(std::time::Duration::from_millis(400u64 * (attempt + 1)))
                    .await;
                continue;
            }
        };
        let status = resp.status();
        if !(status.is_success()) {
            last_err = format!("HTTP {}", status);
            tokio::time::sleep(std::time::Duration::from_millis(400u64 * (attempt + 1)))
                .await;
            continue;
        }
        let resuming = resume_from > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut file = if resuming {
            match std::fs::OpenOptions::new().append(true).open(&part_path) {
                Ok(f) => f,
                Err(e) => return Err(format!("append {}: {}", part_path.display(), e)),
            }
        } else {
            match std::fs::File::create(&part_path) {
                Ok(f) => f,
                Err(e) => return Err(format!("create {}: {}", part_path.display(), e)),
            }
        };
        let mut hasher = Sha1::new();
        if resuming {
            match std::fs::File::open(&part_path) {
                Ok(mut pf) => {
                    let mut buf = vec![0u8; 256 * 1024];
                    loop {

                        match std::io::Read::read(&mut pf, &mut buf) {
                            Ok(0) => break,
                            Ok(n) => hasher.update(&buf[..n]),
                            Err(_) => break,
                        }
                    }
                }
                Err(_) => {
                    let _ = std::fs::File::create(&part_path);
                }
            }
        }
        let mut stream = resp.bytes_stream();
        let mut failed = false;
        while let Some(chunk) = stream.next().await {
            if job.cancelled() {
                drop(file);
                return Err("cancelled".to_string());
            }
            match chunk {
                Ok(bytes) => {
                    if let Err(e) = file.write_all(&bytes) {
                        failed = true;
                        last_err = format!("write: {}", e);
                        break;
                    }
                    hasher.update(&bytes);
                    tracker.add_downloaded(bytes.len() as u64);
                    tracker.emit(app, job_id, label, "download", &name, false);
                }
                Err(e) => {
                    failed = true;
                    last_err = format!("stream: {}", e);
                    break;
                }
            }
        }
        drop(file);
        if !failed {
            if let Some(expected) = &item.sha1 {
                let actual = hex::encode(hasher.finalize());
                if actual != *expected {
                    last_err = format!("sha1 mismatch: {}", name);
                    let _ = std::fs::remove_file(&part_path);
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    continue;
                }
            }
            std::fs::rename(&part_path, &item.dest).map_err(|e| e.to_string())?;
            tracker.file_done();
            return Ok(());
        }
        if attempt == 2 {
            let _ = std::fs::remove_file(&part_path);
        }
        tokio::time::sleep(std::time::Duration::from_millis(400u64 * (attempt + 1))).await;
    }
    Err(last_err)
}

pub async fn download_all(
    app: &tauri::AppHandle,
    job: &Arc<JobState>,
    job_id: &str,
    label: &str,
    items: Vec<DownloadItem>,
) -> Result<(), String> {
    if items.is_empty() {
        return Ok(());
    }
    let sizes = probe_sizes(&items).await;
    let total_bytes: u64 = items
        .iter()
        .enumerate()
        .map(|(i, it)| it.size.unwrap_or(sizes[i]))
        .sum();
    let tracker = Arc::new(Tracker::new(total_bytes, items.len() as u32));
    tracker.emit(app, job_id, label, "prepare", "", true);

    let mut pending = Vec::new();
    for (i, item) in items.into_iter().enumerate() {
        if file_valid(&item) {
            let existing = item.size.unwrap_or(sizes[i]);
            tracker.add_downloaded(existing);
            tracker.file_done();
        } else {
            pending.push(item);
        }
    }
    tracker.emit(app, job_id, label, "download", "", true);

    let app = Arc::new(app.clone());
    let mut stream = futures_util::stream::iter(pending.into_iter().map(|it| {
        let tracker = tracker.clone();
        let app = app.clone();
        let job_id = job_id.to_string();
        let label = label.to_string();
        async move {
            download_one(&app, job, &tracker, &job_id, &label, &it)
                .await
                .map_err(|e| (it, e))
        }
    }))
    .buffer_unordered(6);

    let mut first_error: Option<String> = None;
    while let Some(result) = stream.next().await {
        match result {
            Ok(()) => {}
            Err((_, e)) if e == "cancelled" => return Err("cancelled".to_string()),
            Err((item, e)) => {
                if first_error.is_none() {
                    first_error = Some(format!("{}: {}", item.dest.display(), e));
                }
                eprintln!("download failed: {} ({})", item.url, e);
            }
        }
    }
    if let Some(err) = first_error {
        return Err(err);
    }
    tracker.emit(app.as_ref(), job_id, label, "done", "", true);
    Ok(())
}

pub fn emit_extract_progress(
    app: &tauri::AppHandle,
    job_id: &str,
    label: &str,
    done: u64,
    total: u64,
) {
    let payload = ProgressPayload {
        job_id: job_id.to_string(),
        label: label.to_string(),
        phase: "extract".to_string(),
        total_bytes: total,
        downloaded_bytes: done,
        remaining_bytes: total.saturating_sub(done),
        files_done: done as u32,
        files_total: total as u32,
        current_file: String::new(),
        speed_bps: 0,
    };
    let _ = app.emit("download-progress", &payload);
}
