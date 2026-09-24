//! Native Windows isolated Codex summary runner.

#![cfg(windows)]

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::thread;
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::Builder;
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

use crate::codex_runner::discover_codex_executable;
use crate::paths::{AppPaths, open_private_file, secure_directory};
use crate::rollout_observer::{TurnPack, redact_sensitive_text};
use crate::store::{MAX_SUMMARY_COMPLETIONS_PER_CLAIM, SummaryClaim};
use crate::summary::{MAX_SUMMARY_DOCUMENT_BYTES, SummaryDocument};
use crate::windows_job::JobObject;

pub const SPARK_MODEL: &str = "gpt-5.6-luna";
pub const SPARK_REASONING_EFFORT: &str = "high";
const MAX_TURN_PACK_BYTES: usize = 64 * 1024;
const MAX_TURN_PACK_TOTAL_BYTES: usize = 1024 * 1024;
const MAX_PROMPT_BYTES: usize = 2 * 1024 * 1024;
const MAX_AUTH_BYTES: u64 = 64 * 1024;
const DISABLED_SPARK_FEATURES: &[&str] = &[
    "plugins", "remote_plugin", "plugin_sharing", "skill_search", "skill_mcp_dependency_install",
    "memories", "goals", "hooks", "apps", "enable_mcp_apps", "shell_snapshot",
    "workspace_dependencies", "multi_agent", "multi_agent_v2", "image_generation",
    "browser_use", "browser_use_external", "browser_use_full_cdp_access", "computer_use",
    "in_app_browser", "artifact", "code_mode", "code_mode_host", "tool_suggest",
    "auth_elicitation", "request_permissions_tool", "external_agent_memory_import",
    "shell_tool", "unified_exec",
];

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum SparkError {
    #[error("Codex CLI is unavailable")]
    CliMissing,
    #[error("Codex authentication is missing or unsafe")]
    Authentication,
    #[error("Spark temporary workspace is unsafe")]
    UnsafeWorkspace,
    #[error("Spark input is invalid or exceeds a fixed bound")]
    InvalidInput,
    #[error("Spark output is invalid")]
    InvalidOutput,
    #[error("Spark output exceeds a fixed bound")]
    OutputTooLarge,
    #[error("Spark invocation timed out")]
    Timeout,
    #[error("Spark invocation was cancelled")]
    Cancelled,
    #[error("Spark summary generation is already running for this task")]
    Busy,
    #[error("Spark process I/O failed")]
    ProcessIo,
    #[error("Spark provider rate limit was reached")]
    RateLimited,
    #[error("Spark provider or transport is temporarily unavailable")]
    ServiceUnavailable,
    #[error("requested Spark model is unavailable")]
    ModelUnavailable,
    #[error("Spark local runtime initialization failed")]
    LocalRuntime,
    #[error("Spark process exited unsuccessfully")]
    ExitFailure,
}

#[derive(Debug, Clone)]
pub struct SparkRunnerConfig {
    pub executable: PathBuf,
    pub auth_path: PathBuf,
    pub temp_root: PathBuf,
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

impl Default for SparkRunnerConfig {
    fn default() -> Self {
        let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
        let codex_home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex"));
        Self {
            executable: discover_codex_executable(),
            auth_path: codex_home.join("auth.json"),
            temp_root: AppPaths::from_home(&home).runtime_directory.join("spark"),
            timeout: Duration::from_secs(5 * 60),
            max_stdout_bytes: 4 * 1024 * 1024,
            max_stderr_bytes: 64 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SparkRunner {
    config: SparkRunnerConfig,
}

impl SparkRunner {
    pub fn new(config: SparkRunnerConfig) -> Self { Self { config } }

    pub fn run(&self, claim: &SummaryClaim, previous_unheard: Option<&SummaryDocument>) -> Result<SummaryDocument, SparkError> {
        self.run_with_cancel(claim, previous_unheard, &AtomicBool::new(false))
    }

    pub fn run_with_cancel(&self, claim: &SummaryClaim, previous_unheard: Option<&SummaryDocument>, cancel: &AtomicBool) -> Result<SummaryDocument, SparkError> {
        validate_claim(claim, previous_unheard)?;
        let mut prompt = build_prompt(claim, previous_unheard)?;
        secure_directory(&self.config.temp_root).map_err(|_| SparkError::UnsafeWorkspace)?;
        let lock_name = hex_digest(claim.task_id.as_bytes());
        let lock_path = self.config.temp_root.join(format!(".task-{lock_name}.lock"));
        let lock = open_private_file(&lock_path).map_err(|_| SparkError::UnsafeWorkspace)?;
        lock.try_lock_exclusive().map_err(|_| SparkError::Busy)?;
        let sweep_lock = open_private_file(&self.config.temp_root.join(".sweep.lock")).map_err(|_| SparkError::UnsafeWorkspace)?;
        sweep_lock.lock_exclusive().map_err(|_| SparkError::UnsafeWorkspace)?;
        sweep_stale_runs(&self.config.temp_root)?;
        let run = Builder::new().prefix("spark-").tempdir_in(&self.config.temp_root).map_err(|_| SparkError::UnsafeWorkspace)?;
        let owner_lock = open_private_file(&run.path().join("owner.lock")).map_err(|_| SparkError::UnsafeWorkspace)?;
        owner_lock.lock_exclusive().map_err(|_| SparkError::UnsafeWorkspace)?;
        let _ = fs2::FileExt::unlock(&sweep_lock);
        let home = run.path().join("home");
        let work = run.path().join("work");
        let temporary = run.path().join("tmp");
        for path in [&home, &work, &temporary] {
            fs::create_dir(path).map_err(|_| SparkError::UnsafeWorkspace)?;
        }
        let auth = read_auth(&self.config.auth_path)?;
        let auth_copy = home.join("auth.json");
        let mut auth_file = OpenOptions::new().write(true).create_new(true).open(&auth_copy).map_err(|_| SparkError::UnsafeWorkspace)?;
        auth_file.write_all(auth.as_slice()).map_err(|_| SparkError::UnsafeWorkspace)?;
        auth_file.sync_all().map_err(|_| SparkError::UnsafeWorkspace)?;
        let _auth_copy_guard = AuthCopyGuard(auth_file);
        let auth_hash: [u8; 32] = Sha256::digest(auth.as_slice()).into();
        drop(auth);
        fs::write(home.join(".sandbox_migration"), b"v1\n").map_err(|_| SparkError::UnsafeWorkspace)?;
        fs::create_dir(home.join("skills")).map_err(|_| SparkError::UnsafeWorkspace)?;
        let schema = run.path().join("summary.schema.json");
        let output = run.path().join("summary.output.json");
        fs::write(&schema, output_schema()).map_err(|_| SparkError::UnsafeWorkspace)?;
        File::create(&output).map_err(|_| SparkError::UnsafeWorkspace)?;
        let output_guard = File::open(&output).map_err(|_| SparkError::UnsafeWorkspace)?;

        let mut command = Command::new(&self.config.executable);
        command.args(["exec", "--model", SPARK_MODEL, "--sandbox", "read-only", "--ephemeral", "--ignore-user-config", "--ignore-rules", "--skip-git-repo-check", "--config", "approval_policy=\"never\"", "--config", "model_reasoning_effort=\"high\"", "--output-schema"]);
        command.arg(&schema);
        for feature in DISABLED_SPARK_FEATURES { command.args(["--disable", feature]); }
        command.arg("--output-last-message").arg(&output).args(["--color", "never", "--cd"]).arg(&work).arg("-");
        command.env_clear().env("CODEX_HOME", &home).env("HOME", &home).env("USERPROFILE", &home).env("TEMP", &temporary).env("TMP", &temporary).env("TMPDIR", &temporary).current_dir(&work).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        for name in ["SystemRoot", "WINDIR", "PATH", "HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY", "NO_PROXY"] {
            if let Some(value) = std::env::var_os(name) { command.env(name, value); }
        }
        let mut child = command.spawn().map_err(|error| if error.kind() == std::io::ErrorKind::NotFound { SparkError::CliMissing } else { SparkError::ProcessIo })?;
        let job = match JobObject::attach(&child) { Ok(job) => job, Err(_) => { let _ = child.kill(); let _ = child.wait(); return Err(SparkError::ProcessIo); } };
        let mut stdin = child.stdin.take().ok_or(SparkError::ProcessIo)?;
        let stdout = child.stdout.take().ok_or(SparkError::ProcessIo)?;
        let stderr = child.stderr.take().ok_or(SparkError::ProcessIo)?;
        let writer = thread::spawn(move || {
            let result = stdin.write_all(prompt.as_slice()).and_then(|_| stdin.flush());
            prompt.zeroize();
            result.is_ok()
        });
        let stdout_limit = self.config.max_stdout_bytes;
        let stderr_limit = self.config.max_stderr_bytes;
        let stdout_reader = thread::spawn(move || read_bounded_pipe(stdout, stdout_limit));
        let stderr_reader = thread::spawn(move || read_bounded_pipe(stderr, stderr_limit));
        let deadline = Instant::now() + self.config.timeout;
        let status = loop {
            if cancel.load(std::sync::atomic::Ordering::Acquire) { job.terminate(); break Err(SparkError::Cancelled); }
            if Instant::now() >= deadline { job.terminate(); break Err(SparkError::Timeout); }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(_) => { job.terminate(); break Err(SparkError::ProcessIo); }
            }
        };
        if status.is_err() { let _ = child.wait(); }
        drop(job);
        let wrote = writer.join().unwrap_or(false);
        let stdout_bytes = stdout_reader.join().unwrap_or(Err(SparkError::ProcessIo))?;
        let stderr_bytes = stderr_reader.join().unwrap_or(Err(SparkError::ProcessIo))?;
        let status = status?;
        if !wrote { return Err(SparkError::ProcessIo); }
        if !status.success() { return Err(classify_stderr(&stderr_bytes)); }
        let _ = stdout_bytes;
        if !crate::windows_paths::same_file_handle(&output_guard, &output).map_err(|_| SparkError::InvalidOutput)? { return Err(SparkError::InvalidOutput); }
        let output_bytes = Zeroizing::new(fs::read(&output).map_err(|_| SparkError::InvalidOutput)?);
        if output_bytes.is_empty() || output_bytes.len() > MAX_SUMMARY_DOCUMENT_BYTES { return Err(SparkError::InvalidOutput); }
        let current_auth = read_auth(&auth_copy)?;
        if Sha256::digest(current_auth.as_slice()).as_slice() != auth_hash { return Err(SparkError::Authentication); }
        let source_auth = read_auth(&self.config.auth_path)?;
        if Sha256::digest(source_auth.as_slice()).as_slice() != auth_hash { return Err(SparkError::Authentication); }
        let mut document = SummaryDocument::parse(&output_bytes).map_err(|_| SparkError::InvalidOutput)?;
        let expected = claim.completions.iter().map(|item| item.completion_id.clone()).collect::<Vec<_>>();
        document.validate_expected_covers(&expected).map_err(|_| SparkError::InvalidOutput)?;
        for text in document.facts.iter_mut().chain(document.pending.iter_mut()).chain(document.decisions.iter_mut()).chain(std::iter::once(&mut document.spoken_text)) {
            let redacted = redact_sensitive_text(text);
            text.zeroize();
            *text = redacted;
        }
        document.validate_expected_covers(&expected).map_err(|_| SparkError::InvalidOutput)?;
        let _ = fs2::FileExt::unlock(&lock);
        let _ = fs2::FileExt::unlock(&owner_lock);
        Ok(document)
    }
}

struct AuthCopyGuard(File);

impl Drop for AuthCopyGuard {
    fn drop(&mut self) {
        let Ok(length) = self.0.metadata().map(|metadata| metadata.len()) else { return; };
        if self.0.seek(SeekFrom::Start(0)).is_err() { return; }
        let zero = [0_u8; 4096];
        let mut remaining = length.min(MAX_AUTH_BYTES);
        while remaining > 0 {
            let count = remaining.min(zero.len() as u64) as usize;
            if self.0.write_all(&zero[..count]).is_err() { return; }
            remaining -= count as u64;
        }
        let _ = self.0.sync_all();
        let _ = self.0.set_len(0);
        let _ = self.0.sync_all();
    }
}

fn sweep_stale_runs(temp_root: &Path) -> Result<(), SparkError> {
    for entry in fs::read_dir(temp_root).map_err(|_| SparkError::UnsafeWorkspace)? {
        let entry = entry.map_err(|_| SparkError::UnsafeWorkspace)?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(SparkError::UnsafeWorkspace)?;
        if name == ".sweep.lock" || name.starts_with(".task-") && name.ends_with(".lock") {
            continue;
        }
        if !name.starts_with("spark-") || !name[6..].bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(SparkError::UnsafeWorkspace);
        }
        let path = entry.path();
        validate_cleanup_tree(&path)?;
        let owner_path = path.join("owner.lock");
        let owner = if owner_path.exists() {
            let file = open_private_file(&owner_path).map_err(|_| SparkError::UnsafeWorkspace)?;
            if file.try_lock_exclusive().is_err() { continue; }
            Some(file)
        } else { None };
        let auth = path.join("home").join("auth.json");
        if auth.exists() {
            let file = OpenOptions::new().read(true).write(true).open(&auth).map_err(|_| SparkError::UnsafeWorkspace)?;
            if !crate::windows_paths::same_file_handle(&file, &auth).map_err(|_| SparkError::UnsafeWorkspace)? { return Err(SparkError::UnsafeWorkspace); }
            drop(AuthCopyGuard(file));
        }
        drop(owner);
        fs::remove_dir_all(path).map_err(|_| SparkError::UnsafeWorkspace)?;
    }
    Ok(())
}

fn validate_cleanup_tree(root: &Path) -> Result<(), SparkError> {
    let mut pending = vec![root.to_path_buf()];
    let mut nodes = 0_usize;
    let mut bytes = 0_u64;
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).map_err(|_| SparkError::UnsafeWorkspace)?;
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 { return Err(SparkError::UnsafeWorkspace); }
        nodes = nodes.checked_add(1).filter(|value| *value <= 4096).ok_or(SparkError::UnsafeWorkspace)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&path).map_err(|_| SparkError::UnsafeWorkspace)? {
                pending.push(entry.map_err(|_| SparkError::UnsafeWorkspace)?.path());
            }
        } else if metadata.is_file() {
            bytes = bytes.checked_add(metadata.len()).filter(|value| *value <= 256 * 1024 * 1024).ok_or(SparkError::UnsafeWorkspace)?;
        } else { return Err(SparkError::UnsafeWorkspace); }
    }
    Ok(())
}

fn read_auth(path: &Path) -> Result<Zeroizing<Vec<u8>>, SparkError> {
    let parent = path.parent().ok_or(SparkError::Authentication)?;
    let directory = crate::windows_paths::open_owned_directory_chain(parent, false).map_err(|_| SparkError::Authentication)?;
    let name = path.file_name().ok_or(SparkError::Authentication)?;
    let mut file = crate::windows_paths::open_file_at(&directory, name, false).map_err(|_| SparkError::Authentication)?;
    if file.metadata().map_err(|_| SparkError::Authentication)?.len() > MAX_AUTH_BYTES { return Err(SparkError::Authentication); }
    let mut bytes = Zeroizing::new(Vec::new());
    file.read_to_end(&mut bytes).map_err(|_| SparkError::Authentication)?;
    if !crate::windows_paths::same_file_handle(&file, path).map_err(|_| SparkError::Authentication)? { return Err(SparkError::Authentication); }
    if bytes.is_empty() { return Err(SparkError::Authentication); }
    Ok(bytes)
}

fn validate_claim(claim: &SummaryClaim, previous: Option<&SummaryDocument>) -> Result<(), SparkError> {
    if uuid::Uuid::parse_str(&claim.task_id).is_err() || claim.generation == 0 || claim.completions.is_empty() || claim.completions.len() > MAX_SUMMARY_COMPLETIONS_PER_CLAIM || claim.previous_unread.is_some() != previous.is_some() || claim.previous_unread.as_ref().is_some_and(|unread| unread.task_id != claim.task_id) { return Err(SparkError::InvalidInput); }
    let mut total = 0_usize;
    for completion in &claim.completions {
        total = total.checked_add(completion.turn_pack.len()).ok_or(SparkError::InvalidInput)?;
        let pack: TurnPack = serde_json::from_str(&completion.turn_pack).map_err(|_| SparkError::InvalidInput)?;
        if uuid::Uuid::parse_str(&completion.completion_id).is_err() || completion.turn_pack.len() > MAX_TURN_PACK_BYTES || total > MAX_TURN_PACK_TOTAL_BYTES || pack.v != 1 || pack.turn_id != completion.completion_id || pack.assistant.len() != 1 || pack.assistant[0].trim().is_empty() { return Err(SparkError::InvalidInput); }
    }
    if previous.is_some_and(|document| document.validate().is_err()) { return Err(SparkError::InvalidInput); }
    Ok(())
}

#[derive(Serialize)]
struct PromptInput<'a> { schema: u8, previous_unheard: Option<&'a SummaryDocument>, new_completions: Vec<AssistantCompletion<'a>> }
#[derive(Serialize)]
struct AssistantCompletion<'a> { completion_id: &'a str, assistant_final: String }

fn build_prompt(claim: &SummaryClaim, previous: Option<&SummaryDocument>) -> Result<Zeroizing<Vec<u8>>, SparkError> {
    const INSTRUCTIONS: &str = "Create a concrete cumulative unread task summary from the JSON input below. Each new completion contains only the authoritative final assistant reply from one completed task turn. Summarize only what those assistant_final fields reported: the user-visible result, still-relevant next work, and explicit decisions. Do not invent or reconstruct user messages, tool calls, intermediate progress, tests, logs, hidden reasoning, or implementation details that are not useful to the user. When previous_unheard is present, use it as cumulative context, then naturally re-summarize the still-relevant old unread content together with every new completion. Do not copy previous spoken_text verbatim and do not narrate a chronological history. Write spoken_text as a natural Simplified Chinese briefing of at most 480 letters, digits, or Han characters. Match its length to the actual information: keep a trivial confirmation to one short sentence; for an ordinary result, retain only the most important conclusion, core numbers, limitations, decisions, and any action the user truly needs; use more length for cumulative unread material only when needed to preserve useful meaning. Never pad toward a target length or paraphrase the whole source. Lead immediately with the newest concrete user-visible result, prioritizing the last new completion, then briefly fold in other material unread outcomes or decisions. End with a next action only when the source contains a real actionable next step; never add a generic closing such as saying work can continue. It must be self-contained and say what was actually completed, what result matters, and any relevant next step or decision; do not merely say that a task is done. Omit test commands, validation mechanics, and implementation detail unless the user must act on them. The TTS reads spoken_text exactly, so never include schema labels, validation notes, section headings, or boilerplate that a person should not hear. Return only the output-schema JSON. covers_new_completions must exactly equal the ordered completion_id values in new_completions. Never emit credentials, hidden reasoning, or local absolute paths.\n";
    let completions = claim.completions.iter().map(|item| {
        let pack: TurnPack = serde_json::from_str(&item.turn_pack).map_err(|_| SparkError::InvalidInput)?;
        Ok(AssistantCompletion { completion_id: &item.completion_id, assistant_final: pack.assistant[0].clone() })
    }).collect::<Result<Vec<_>, SparkError>>()?;
    let input = PromptInput { schema: 1, previous_unheard: previous, new_completions: completions };
    let mut bytes = Zeroizing::new(INSTRUCTIONS.as_bytes().to_vec());
    serde_json::to_writer(&mut *bytes, &input).map_err(|_| SparkError::InvalidInput)?;
    if bytes.len() > MAX_PROMPT_BYTES { return Err(SparkError::InvalidInput); }
    Ok(bytes)
}

fn output_schema() -> String {
    serde_json::json!({"type":"object","additionalProperties":false,"properties":{"schema":{"type":"integer","const":1},"facts":{"type":"array","maxItems":32,"items":{"type":"string"}},"pending":{"type":"array","maxItems":32,"items":{"type":"string"}},"decisions":{"type":"array","maxItems":32,"items":{"type":"string"}},"spoken_text":{"type":"string"},"covers_new_completions":{"type":"array","maxItems":32,"items":{"type":"string"}}},"required":["schema","facts","pending","decisions","spoken_text","covers_new_completions"]}).to_string()
}

fn hex_digest(bytes: &[u8]) -> String { Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect() }

fn read_bounded_pipe(mut reader: impl Read, limit: usize) -> Result<Zeroizing<Vec<u8>>, SparkError> {
    let mut bytes = Zeroizing::new(Vec::new());
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk).map_err(|_| SparkError::ProcessIo)?;
        if read == 0 { return Ok(bytes); }
        if bytes.len().checked_add(read).is_none_or(|next| next > limit) { return Err(SparkError::OutputTooLarge); }
        bytes.extend_from_slice(&chunk[..read]);
    }
}

fn classify_stderr(stderr: &[u8]) -> SparkError {
    let lower = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if lower.contains("authentication") || lower.contains("not logged in") || lower.contains("401") { SparkError::Authentication }
    else if lower.contains("429") || lower.contains("rate limit") { SparkError::RateLimited }
    else if lower.contains("model") && (lower.contains("unavailable") || lower.contains("not found")) { SparkError::ModelUnavailable }
    else if lower.contains("503") || lower.contains("service unavailable") { SparkError::ServiceUnavailable }
    else if lower.contains("permission denied") || lower.contains("failed to initialize") { SparkError::LocalRuntime }
    else { SparkError::ExitFailure }
}
