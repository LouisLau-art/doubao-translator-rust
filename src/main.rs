use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use dotenvy::dotenv;
use lru::LruCache;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    env,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tower_http::{
    cors::CorsLayer,
    services::ServeDir,
};
use axum::routing::get_service;
use tower_http::services::ServeFile;

#[derive(Clone)]
struct AppState {
    config: Config,
    client: Client,
    cache: Cache,
    limiter: RateLimiter,
}

#[derive(Clone)]
struct Config {
    api_key: String,
    api_url: String,
    port: u16,
    cache_ttl: Duration,
    cache_max_size: usize,
    max_text_length: usize,
    rate_limit_rpm: usize,
}

#[derive(Debug, Deserialize)]
struct TranslateRequest {
    text: String,
    source: Option<String>,
    target: String,
}

#[derive(Serialize)]
struct TranslateResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cached: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
struct DoubaoRequest {
    model: String,
    input: Vec<DoubaoInputMessage>,
}

#[derive(Serialize)]
struct DoubaoInputMessage {
    role: String,
    content: Vec<DoubaoContent>,
}

#[derive(Serialize)]
struct DoubaoContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    translation_options: Option<TranslationOptions>,
}

#[derive(Serialize)]
struct TranslationOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    source_language: Option<String>,
    target_language: String,
}

#[derive(Clone)]
struct Cache {
    ttl: Duration,
    inner: Arc<Mutex<LruCache<String, CacheEntry>>>,
}

#[derive(Clone)]
struct CacheEntry {
    value: String,
    expires_at: Instant,
}

#[derive(Clone)]
struct RateLimiter {
    window: Duration,
    max: usize,
    hits: Arc<Mutex<VecDeque<Instant>>>,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let config = match load_config() {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("Config error: {err}");
            std::process::exit(1);
        }
    };

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build HTTP client");

    let cache = Cache::new(config.cache_max_size, config.cache_ttl);
    let limiter = RateLimiter::new(Duration::from_secs(60), config.rate_limit_rpm);

    let state = AppState {
        config,
        client,
        cache,
        limiter,
    };

    let static_service = ServeDir::new("static");
    let libs_service = ServeDir::new("static/libs");

    let app = Router::new()
        .route("/api/translate", post(translate_handler))
        .route("/api/languages", get(languages_handler))
        .route("/api/health", get(health_handler))
        .nest_service("/static", static_service)
        .nest_service("/libs", libs_service)
        .route("/", get_service(ServeFile::new("static/index.html")))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", state.config.port);
    println!("Server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind address");
    axum::serve(listener, app)
        .await
        .expect("server error");
}

async fn translate_handler(
    State(state): State<AppState>,
    Json(payload): Json<TranslateRequest>,
) -> (StatusCode, Json<TranslateResponse>) {
    if !state.limiter.allow().await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(TranslateResponse {
                success: false,
                text: None,
                cached: None,
                error: Some("请求过于频繁，请稍后再试".to_string()),
            }),
        );
    }

    let source = payload.source.as_deref().filter(|s| !s.is_empty());

    let text_len = payload.text.chars().count();
    if text_len == 0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(TranslateResponse {
                success: false,
                text: None,
                cached: None,
                error: Some("文本不能为空".to_string()),
            }),
        );
    }
    if text_len > state.config.max_text_length {
        return (
            StatusCode::BAD_REQUEST,
            Json(TranslateResponse {
                success: false,
                text: None,
                cached: None,
                error: Some(format!("文本长度超过限制（最大{}字符）", state.config.max_text_length)),
            }),
        );
    }

    if payload.target.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(TranslateResponse {
                success: false,
                text: None,
                cached: None,
                error: Some("目标语言不能为空".to_string()),
            }),
        );
    }

    let cache_key = build_cache_key(&payload.text, source, &payload.target);
    if let Some(cached) = state.cache.get(&cache_key).await {
        return (
            StatusCode::OK,
            Json(TranslateResponse {
                success: true,
                text: Some(cached),
                cached: Some(true),
                error: None,
            }),
        );
    }

    let chunks = split_text(&payload.text, 800);
    let mut results = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        match translate_chunk(&state, &chunk, source, &payload.target).await {
            Ok(text) => results.push(text),
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(TranslateResponse {
                        success: false,
                        text: None,
                        cached: None,
                        error: Some(format!("翻译失败: {err}")),
                    }),
                );
            }
        }
    }

    let final_text = results.join("\n");
    state.cache.set(cache_key, final_text.clone()).await;

    (
        StatusCode::OK,
        Json(TranslateResponse {
            success: true,
            text: Some(final_text),
            cached: Some(false),
            error: None,
        }),
    )
}

async fn translate_chunk(
    state: &AppState,
    text: &str,
    source: Option<&str>,
    target: &str,
) -> Result<String, String> {
    let req_body = DoubaoRequest {
        model: "doubao-seed-translation-250915".to_string(),
        input: vec![DoubaoInputMessage {
            role: "user".to_string(),
            content: vec![DoubaoContent {
                content_type: "input_text".to_string(),
                text: text.to_string(),
                translation_options: Some(TranslationOptions {
                    source_language: source.map(|s| s.to_string()),
                    target_language: target.to_string(),
                }),
            }],
        }],
    };

    let resp = state
        .client
        .post(&state.config.api_url)
        .bearer_auth(&state.config.api_key)
        .json(&req_body)
        .send()
        .await
        .map_err(|e| format!("HTTP请求失败: {e}"))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;

    if !status.is_success() {
        return Err(format!("API错误 {}: {}", status.as_u16(), body));
    }

    parse_doubao_response(&body).map_err(|e| format!("响应解析失败: {e}"))
}

fn parse_doubao_response(body: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;

    if value.get("status").and_then(|v| v.as_str()) == Some("completed") {
        if let Some(output) = value.get("output").and_then(|v| v.as_array()) {
            for item in output {
                let is_message = item.get("type").and_then(|v| v.as_str()) == Some("message");
                let is_assistant = item.get("role").and_then(|v| v.as_str()) == Some("assistant");
                if !is_message || !is_assistant {
                    continue;
                }
                if let Some(content) = item.get("content").and_then(|v| v.as_array()) {
                    for part in content {
                        let is_output = part.get("type").and_then(|v| v.as_str()) == Some("output_text");
                        if is_output {
                            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                                return Ok(text.to_string());
                            }
                        }
                    }
                }
            }
        }
        return Err("new format missing output_text".to_string());
    }

    if let Some(choices) = value.get("choices").and_then(|v| v.as_array()) {
        if let Some(choice) = choices.first() {
            if let Some(content) = choice
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|v| v.as_str())
            {
                return Ok(content.to_string());
            }
        }
    }

    Err("unknown response format".to_string())
}

async fn languages_handler() -> Json<Value> {
    Json(json!({
        "success": true,
        "languages": {
            "zh": "中文（简体）",
            "zh-Hant": "中文（繁体）",
            "en": "英语",
            "ja": "日语",
            "ko": "韩语",
            "de": "德语",
            "fr": "法语",
            "es": "西班牙语",
            "it": "意大利语",
            "pt": "葡萄牙语",
            "ru": "俄语",
            "th": "泰语",
            "vi": "越南语",
            "ar": "阿拉伯语"
        }
    }))
}

async fn health_handler() -> Json<Value> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Json(json!({ "status": "healthy", "time": now }))
}

impl Cache {
    fn new(max_size: usize, ttl: Duration) -> Self {
        let max = NonZeroUsize::new(max_size.max(1)).unwrap();
        Self {
            ttl,
            inner: Arc::new(Mutex::new(LruCache::new(max))),
        }
    }

    async fn get(&self, key: &str) -> Option<String> {
        let mut cache = self.inner.lock().await;
        if let Some(entry) = cache.get(key) {
            if Instant::now() <= entry.expires_at {
                return Some(entry.value.clone());
            }
        }
        cache.pop(key);
        None
    }

    async fn set(&self, key: String, value: String) {
        let entry = CacheEntry {
            value,
            expires_at: Instant::now() + self.ttl,
        };
        let mut cache = self.inner.lock().await;
        cache.put(key, entry);
    }
}

impl RateLimiter {
    fn new(window: Duration, max: usize) -> Self {
        Self {
            window,
            max: max.max(1),
            hits: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    async fn allow(&self) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().await;
        while let Some(front) = hits.front() {
            if now.duration_since(*front) > self.window {
                hits.pop_front();
            } else {
                break;
            }
        }
        if hits.len() >= self.max {
            return false;
        }
        hits.push_back(now);
        true
    }
}

fn build_cache_key(text: &str, source: Option<&str>, target: &str) -> String {
    let base = format!("{}|{}|{}", source.unwrap_or(""), target, text);
    format!("{:x}", md5::compute(base))
}

fn split_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for paragraph in paragraphs {
        let para_len = paragraph.chars().count();
        if para_len > max_chars {
            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
                current_len = 0;
            }
            for part in split_by_chars(paragraph, max_chars) {
                chunks.push(part);
            }
            continue;
        }

        let extra = if current.is_empty() { 0 } else { 2 };
        if !current.is_empty() && current_len + extra + para_len > max_chars {
            chunks.push(current);
            current = paragraph.to_string();
            current_len = para_len;
        } else {
            if !current.is_empty() {
                current.push_str("\n\n");
                current_len += 2;
            }
            current.push_str(paragraph);
            current_len += para_len;
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn split_by_chars(text: &str, max_chars: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut count = 0usize;

    for ch in text.chars() {
        current.push(ch);
        count += 1;
        if count >= max_chars {
            parts.push(current);
            current = String::new();
            count = 0;
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

fn load_config() -> Result<Config, String> {
    let api_key = env::var("ARK_API_KEY").map_err(|_| "ARK_API_KEY not set".to_string())?;
    let api_url = env::var("ARK_API_URL")
        .unwrap_or_else(|_| "https://ark.cn-beijing.volces.com/api/v3/responses".to_string());

    let port = env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5000);

    let cache_ttl = env_usize("CACHE_TTL", 3600);
    let cache_max_size = env_usize("CACHE_MAX_SIZE", 1000);
    let max_text_length = env_usize("MAX_TEXT_LENGTH", 5000);
    let rate_limit_rpm = env_usize("RATE_LIMIT_RPM", 30);

    Ok(Config {
        api_key,
        api_url,
        port,
        cache_ttl: Duration::from_secs(cache_ttl as u64),
        cache_max_size,
        max_text_length,
        rate_limit_rpm,
    })
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
