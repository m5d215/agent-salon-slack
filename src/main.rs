mod safety;

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use safety::{Classification, Classifier};
use serde::{Deserialize, Serialize};
use slack_morphism::prelude::*;
use std::{collections::HashMap, sync::Arc, sync::OnceLock, time::Duration};
use tokio::net::TcpListener;
use tracing::{error, info, warn};

#[derive(Clone)]
struct Config {
    bot_token: SlackApiToken,
    app_token: SlackApiToken,
    salon_notify_url: String,
    salon_label: String,
    salon_target: String,
    http_bind: String,
    http_port: u16,
    ollama_url: String,
    ollama_model: String,
    injection_block_threshold: f64,
    injection_warn_threshold: f64,
    injection_timeout: Duration,
}

impl Config {
    fn resolve(file: &HashMap<String, String>) -> Result<Self, String> {
        let req = |k: &str| cfg_var(file, k).ok_or_else(|| format!("{k}: not set"));
        let opt_or =
            |k: &str, default: &str| cfg_var(file, k).unwrap_or_else(|| default.to_string());
        let bot = req("SLACK_BOT_TOKEN")?;
        let app = req("SLACK_APP_TOKEN")?;
        let salon_notify_url = req("AGENT_SALON_URL")?;
        let salon_label = req("AGENT_SALON_LABEL")?;
        let salon_target = req("AGENT_SALON_TARGET")?;
        let port: u16 = opt_or("AGENT_SALON_SLACK_HTTP_PORT", "8765")
            .parse()
            .map_err(|e| format!("AGENT_SALON_SLACK_HTTP_PORT: {e}"))?;
        let http_bind = opt_or("AGENT_SALON_SLACK_HTTP_BIND", "127.0.0.1");
        let ollama_url = opt_or("OLLAMA_URL", "http://localhost:11434");
        let ollama_model = opt_or("OLLAMA_MODEL", "llama-guard3:1b");
        let injection_block_threshold: f64 = opt_or("INJECTION_BLOCK_THRESHOLD", "0.7")
            .parse()
            .map_err(|e| format!("INJECTION_BLOCK_THRESHOLD: {e}"))?;
        let injection_warn_threshold: f64 = opt_or("INJECTION_WARN_THRESHOLD", "0.5")
            .parse()
            .map_err(|e| format!("INJECTION_WARN_THRESHOLD: {e}"))?;
        let injection_timeout_secs: u64 = opt_or("INJECTION_TIMEOUT_SECS", "30")
            .parse()
            .map_err(|e| format!("INJECTION_TIMEOUT_SECS: {e}"))?;
        Ok(Self {
            bot_token: SlackApiToken::new(SlackApiTokenValue::from(bot)),
            app_token: SlackApiToken::new(SlackApiTokenValue::from(app)),
            salon_notify_url,
            salon_label,
            salon_target,
            http_bind,
            http_port: port,
            ollama_url,
            ollama_model,
            injection_block_threshold,
            injection_warn_threshold,
            injection_timeout: Duration::from_secs(injection_timeout_secs),
        })
    }
}

/// Resolve a config value, preferring the live process environment over
/// any value loaded from the config file.
fn cfg_var(file: &HashMap<String, String>, key: &str) -> Option<String> {
    std::env::var(key).ok().or_else(|| file.get(key).cloned())
}

/// Read `AGENT_SALON_SLACK_CONFIG` and parse the file at that path.
/// Returns an empty map when the env var is unset (no config file used)
/// or when the file is missing. The path is exposed via env so platform
/// installers (e.g. the Homebrew formula) can point at
/// `${HOMEBREW_PREFIX}/etc/agent-salon-slack.conf` without code changes.
fn load_config_file() -> HashMap<String, String> {
    let Ok(path) = std::env::var("AGENT_SALON_SLACK_CONFIG") else {
        return HashMap::new();
    };
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let map = parse_config(&s);
            info!(
                kind = "startup.config_file_loaded",
                path = %path,
                count = map.len(),
                "loaded settings from config file"
            );
            map
        }
        Err(e) => {
            warn!(
                kind = "startup.config_file_skipped",
                path = %path,
                error = %e,
                "skipped config file"
            );
            HashMap::new()
        }
    }
}

/// Parse a `KEY=VALUE` config file. Lines starting with `#` and blank
/// lines are skipped. Keys with no `=` are skipped. Surrounding double
/// quotes around the value (`KEY="value"`) are stripped. Whitespace
/// around the key and around the value (outside the quotes) is trimmed.
fn parse_config(s: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in s.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        let v = v.trim();
        let v = if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
            &v[1..v.len() - 1]
        } else {
            v
        };
        out.insert(k.to_string(), v.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config_basics() {
        let s = r#"
# comment
KEY=value
QUOTED="double quoted"
WITH_SPACES   =   spaced
EMPTY_VALUE=
=ignored_no_key
no_equals_line
"#;
        let m = parse_config(s);
        assert_eq!(m.get("KEY").map(String::as_str), Some("value"));
        assert_eq!(m.get("QUOTED").map(String::as_str), Some("double quoted"));
        assert_eq!(m.get("WITH_SPACES").map(String::as_str), Some("spaced"));
        assert_eq!(m.get("EMPTY_VALUE").map(String::as_str), Some(""));
        assert!(!m.contains_key(""));
        assert!(!m.contains_key("no_equals_line"));
    }
}

#[derive(Clone)]
struct SharedState {
    slack: Arc<SlackHyperClient>,
    config: Arc<Config>,
    http: reqwest::Client,
    self_bot_id: Option<SlackBotId>,
    self_user_id: Option<SlackUserId>,
    classifier: Arc<Classifier>,
}

static SHARED: OnceLock<SharedState> = OnceLock::new();

#[derive(Serialize)]
struct SalonPayload<'a> {
    target: &'a str,
    content: String,
    meta: SalonMeta,
}

#[derive(Serialize)]
struct SalonMeta {
    kind: &'static str,
}

async fn notify_salon(
    state: &SharedState,
    content: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let payload = SalonPayload {
        target: &state.config.salon_target,
        content: content.clone(),
        meta: SalonMeta { kind: "slack" },
    };
    state
        .http
        .post(&state.config.salon_notify_url)
        .query(&[("label", &state.config.salon_label)])
        .json(&payload)
        .send()
        .await?
        .error_for_status()?;
    info!(
        kind = "salon.forwarded",
        target = %state.config.salon_target,
        url = %state.config.salon_notify_url,
        content = %content,
        "forwarded to agent-salon"
    );
    Ok(())
}

/// Source-agnostic representation of a Slack event after triage.
struct ParsedEvent {
    event: &'static str,
    channel: String,
    user: Option<String>,
    bot_id: Option<String>,
    username: Option<String>,
    ts: String,
    thread_ts: Option<String>,
    reply_thread_ts: String,
    text: String,
}

fn parse_message_event(
    m: &SlackMessageEvent,
    self_bot_id: Option<&SlackBotId>,
) -> Option<ParsedEvent> {
    // Whitelist subtypes — pass through human-equivalent message events,
    // drop edits/deletes/system events.
    match &m.subtype {
        None => {}
        Some(SlackMessageEventType::BotMessage) => {}
        Some(SlackMessageEventType::ThreadBroadcast) => {}
        Some(SlackMessageEventType::FileShare) => {}
        Some(SlackMessageEventType::MeMessage) => {}
        Some(_) => return None,
    }

    if m.hidden.unwrap_or(false) {
        return None;
    }

    // Drop our own bot's posts to prevent infinite loops.
    if let (Some(sender_bot_id), Some(self_bot_id)) = (&m.sender.bot_id, self_bot_id) {
        if sender_bot_id == self_bot_id {
            return None;
        }
    }

    let text = m.content.as_ref()?.text.as_ref()?;
    if text.trim().is_empty() {
        return None;
    }

    let channel = m.origin.channel.as_ref()?;
    let user = m.sender.user.as_ref().map(|u| u.to_string());
    let bot_id = m.sender.bot_id.as_ref().map(|b| b.to_string());

    // Need at least one identifier for the sender.
    if user.is_none() && bot_id.is_none() {
        return None;
    }

    let reply_thread_ts = m.origin.thread_ts.as_ref().unwrap_or(&m.origin.ts);
    Some(ParsedEvent {
        event: "message",
        channel: channel.to_string(),
        user,
        bot_id,
        username: m.sender.username.clone(),
        ts: m.origin.ts.to_string(),
        thread_ts: m.origin.thread_ts.as_ref().map(|t| t.to_string()),
        reply_thread_ts: reply_thread_ts.to_string(),
        text: text.clone(),
    })
}

/// Source-agnostic representation of a Slack reaction event.
struct ParsedReaction {
    event: &'static str,
    channel: Option<String>,
    user: String,
    reaction: String,
    item_type: &'static str,
    item_user: Option<String>,
    item_ts: Option<String>,
    event_ts: String,
}

fn parse_reaction_event(
    event: &'static str,
    user: &SlackUserId,
    reaction: &SlackReactionName,
    item_user: Option<&SlackUserId>,
    item: &SlackReactionsItem,
    event_ts: &SlackTs,
    self_user_id: Option<&SlackUserId>,
) -> Option<ParsedReaction> {
    // Drop reactions added/removed by ourselves to avoid feedback loops
    // and to keep the salon stream free of our own bookkeeping.
    if let Some(self_id) = self_user_id {
        if user == self_id {
            return None;
        }
    }
    let (channel, item_type, item_ts) = match item {
        SlackReactionsItem::Message(m) => (
            m.origin.channel.as_ref().map(|c| c.to_string()),
            "message",
            Some(m.origin.ts.to_string()),
        ),
        SlackReactionsItem::File(_) => (None, "file", None),
    };
    Some(ParsedReaction {
        event,
        channel,
        user: user.to_string(),
        reaction: reaction.to_string(),
        item_type,
        item_user: item_user.map(|u| u.to_string()),
        item_ts,
        event_ts: event_ts.to_string(),
    })
}

fn render_reaction_json(p: &ParsedReaction) -> String {
    let value = serde_json::json!({
        "event": p.event,
        "channel": p.channel,
        "user": p.user,
        "reaction": p.reaction,
        "item_type": p.item_type,
        "item_user": p.item_user,
        "item_ts": p.item_ts,
        "event_ts": p.event_ts,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
}

fn render_event_json(p: &ParsedEvent, safety: Option<&Classification>) -> String {
    let value = serde_json::json!({
        "event": p.event,
        "channel": p.channel,
        "user": p.user,
        "bot_id": p.bot_id,
        "username": p.username,
        "ts": p.ts,
        "thread_ts": p.thread_ts,
        "reply_thread_ts": p.reply_thread_ts,
        "text": p.text,
        "safety": safety,
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
}

async fn handle_push_event(
    event: SlackPushEventCallback,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = match SHARED.get() {
        Some(s) => s,
        None => {
            error!(
                kind = "internal.shared_state_uninitialized",
                "shared state not initialized"
            );
            return Ok(());
        }
    };

    match event.event {
        SlackEventCallbackBody::Message(m) => {
            handle_message_event(state, m).await;
        }
        SlackEventCallbackBody::ReactionAdded(r) => {
            handle_reaction_event(
                state,
                "reaction_added",
                &r.user,
                &r.reaction,
                r.item_user.as_ref(),
                &r.item,
                &r.event_ts,
            )
            .await;
        }
        SlackEventCallbackBody::ReactionRemoved(r) => {
            handle_reaction_event(
                state,
                "reaction_removed",
                &r.user,
                &r.reaction,
                r.item_user.as_ref(),
                &r.item,
                &r.event_ts,
            )
            .await;
        }
        other => {
            info!(
                kind = "slack.other_event",
                event = %format!("{other:?}"),
                "non-handled slack event received"
            );
        }
    }
    Ok(())
}

async fn handle_message_event(state: &SharedState, m: SlackMessageEvent) {
    let text = m
        .content
        .as_ref()
        .and_then(|c| c.text.clone())
        .unwrap_or_default();
    let channel = m
        .origin
        .channel
        .as_ref()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let ts = m.origin.ts.to_string();
    let thread_ts = m
        .origin
        .thread_ts
        .as_ref()
        .map(|t| t.to_string())
        .unwrap_or_default();
    let user = m
        .sender
        .user
        .as_ref()
        .map(|u| u.to_string())
        .unwrap_or_default();
    let bot_id = m
        .sender
        .bot_id
        .as_ref()
        .map(|b| b.to_string())
        .unwrap_or_default();
    let subtype = m
        .subtype
        .as_ref()
        .map(|s| format!("{s:?}"))
        .unwrap_or_default();
    info!(
        kind = "slack.event_received",
        channel = %channel,
        user = %user,
        bot_id = %bot_id,
        ts = %ts,
        thread_ts = %thread_ts,
        subtype = %subtype,
        text = %text,
        "slack message event received"
    );
    let parsed = match parse_message_event(&m, state.self_bot_id.as_ref()) {
        Some(p) => p,
        None => {
            info!(
                kind = "slack.message_dropped",
                channel = %channel,
                ts = %ts,
                subtype = %subtype,
                "slack message dropped at triage"
            );
            return;
        }
    };

    // Run injection classifier on the parsed text.
    let classification = state.classifier.classify(&parsed.text).await;
    info!(
        kind = "safety.classified",
        score = classification.score,
        label = %classification.label,
        model = %classification.model,
        fallback = classification.fallback,
        reason = classification.reason.as_deref().unwrap_or(""),
        "safety classification produced"
    );

    if classification.score >= state.config.injection_block_threshold {
        warn!(
            kind = "slack.message_blocked",
            score = classification.score,
            label = %classification.label,
            model = %classification.model,
            fallback = classification.fallback,
            reason = classification.reason.as_deref().unwrap_or(""),
            channel = %parsed.channel,
            user = parsed.user.as_deref().unwrap_or(""),
            bot_id = parsed.bot_id.as_deref().unwrap_or(""),
            username = parsed.username.as_deref().unwrap_or(""),
            ts = %parsed.ts,
            thread_ts = parsed.thread_ts.as_deref().unwrap_or(""),
            reply_thread_ts = %parsed.reply_thread_ts,
            text = %parsed.text,
            "slack message blocked by safety classifier"
        );
        if let Err(e) = post_safety_alert(state, &parsed, &classification).await {
            warn!(
                kind = "slack.alert_failed",
                error = %format!("{e:?}"),
                "failed to post safety alert"
            );
        }
        return;
    }

    let safety_annotation = if classification.score >= state.config.injection_warn_threshold {
        Some(classification)
    } else {
        None
    };

    let content = render_event_json(&parsed, safety_annotation.as_ref());
    if let Err(e) = notify_salon(state, content).await {
        warn!(
            kind = "salon.forward_failed",
            target = %state.config.salon_target,
            url = %state.config.salon_notify_url,
            error = %format!("{e:?}"),
            "failed to forward to agent-salon"
        );
    }
}

async fn handle_reaction_event(
    state: &SharedState,
    event: &'static str,
    user: &SlackUserId,
    reaction: &SlackReactionName,
    item_user: Option<&SlackUserId>,
    item: &SlackReactionsItem,
    event_ts: &SlackTs,
) {
    info!(
        kind = "slack.reaction_received",
        event = %event,
        user = %user,
        reaction = %reaction.0,
        item_user = item_user.map(|u| u.to_string()).unwrap_or_default(),
        event_ts = %event_ts,
        "slack reaction event received"
    );
    let parsed = match parse_reaction_event(
        event,
        user,
        reaction,
        item_user,
        item,
        event_ts,
        state.self_user_id.as_ref(),
    ) {
        Some(p) => p,
        None => {
            info!(
                kind = "slack.reaction_dropped",
                event = %event,
                user = %user,
                reaction = %reaction.0,
                "slack reaction dropped at triage"
            );
            return;
        }
    };

    let content = render_reaction_json(&parsed);
    if let Err(e) = notify_salon(state, content).await {
        warn!(
            kind = "salon.forward_failed",
            target = %state.config.salon_target,
            url = %state.config.salon_notify_url,
            error = %format!("{e:?}"),
            "failed to forward to agent-salon"
        );
    }
}

/// Post a brief alert to the same thread as the offending message.
/// Original text is intentionally NOT included to avoid re-feeding it back
/// into agent-salon (the bot's own posts are filtered, but this is defense
/// in depth — also keeps log readable).
async fn post_safety_alert(
    state: &SharedState,
    parsed: &ParsedEvent,
    _classification: &Classification,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Detection metadata (score / model / fallback) is intentionally not posted
    // to Slack — see the structured `kind=slack.message_blocked` log line for
    // details.
    let alert = ":warning: prompt injection detected".to_string();
    let session = state.slack.open_session(&state.config.bot_token);
    let req = SlackApiChatPostMessageRequest::new(
        SlackChannelId::from(parsed.channel.clone()),
        SlackMessageContent::new().with_text(alert),
    )
    .with_thread_ts(SlackTs::from(parsed.reply_thread_ts.clone()));
    session.chat_post_message(&req).await?;
    Ok(())
}

fn on_error(
    err: Box<dyn std::error::Error + Send + Sync>,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) -> HttpStatusCode {
    error!(
        kind = "callback.error",
        error = %format!("{err:?}"),
        "slack callback error"
    );
    HttpStatusCode::OK
}

#[derive(Deserialize)]
struct PostRequest {
    channel: String,
    thread_ts: Option<String>,
    text: String,
}

async fn post_handler(
    State(state): State<SharedState>,
    Json(req): Json<PostRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let channel = req.channel.clone();
    let thread_ts = req.thread_ts.clone().unwrap_or_default();
    info!(
        kind = "post.requested",
        channel = %channel,
        thread_ts = %thread_ts,
        text = %req.text,
        "post request received"
    );
    let session = state.slack.open_session(&state.config.bot_token);
    let mut req_msg = SlackApiChatPostMessageRequest::new(
        SlackChannelId::from(req.channel),
        SlackMessageContent::new().with_text(req.text),
    );
    if let Some(t) = req.thread_ts {
        req_msg = req_msg.with_thread_ts(SlackTs::from(t));
    }
    session.chat_post_message(&req_msg).await.map_err(|e| {
        error!(
            kind = "slack.post_failed",
            channel = %channel,
            thread_ts = %thread_ts,
            error = %format!("{e:?}"),
            "chat.postMessage failed"
        );
        (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
    })?;
    info!(
        kind = "slack.message_posted",
        channel = %channel,
        thread_ts = %thread_ts,
        "posted to slack"
    );
    Ok(StatusCode::OK)
}

async fn run_http_server(state: SharedState) {
    let bind = state.config.http_bind.clone();
    let port = state.config.http_port;
    let app = Router::new()
        .route("/post", post(post_handler))
        .with_state(state);
    let listener = TcpListener::bind((bind.as_str(), port))
        .await
        .expect("bind http");
    let local_addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{bind}:{port}"));
    info!(
        kind = "startup.http_listening",
        addr = %local_addr,
        "http server listening"
    );
    axum::serve(listener, app).await.expect("http serve");
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agent_salon_slack=info,slack_morphism=info".into()),
        )
        .init();

    // Route panics through tracing so the viewer sees them.
    std::panic::set_hook(Box::new(|info| {
        error!(kind = "internal.panic", info = %format!("{info}"), "panic");
    }));

    if let Err(e) = run().await {
        error!(
            kind = "startup.fatal",
            error = %format!("{e:?}"),
            "startup failed"
        );
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = load_config_file();
    let config = Arc::new(Config::resolve(&file)?);
    let slack_client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));
    let http = reqwest::Client::new();

    // Identify self to allow other bots' messages while still dropping our own
    // posts (which come back as bot_message events).
    let auth = slack_client
        .open_session(&config.bot_token)
        .auth_test()
        .await?;
    let self_bot_id = auth.bot_id.clone();
    let self_user_id = Some(auth.user_id.clone());
    info!(
        kind = "startup.auth_test_ok",
        user_id = %auth.user_id,
        bot_id = self_bot_id.as_ref().map(|b| b.to_string()).unwrap_or_default(),
        team = %auth.team,
        "auth test ok"
    );

    let classifier = Arc::new(Classifier {
        ollama_url: config.ollama_url.clone(),
        ollama_model: config.ollama_model.clone(),
        timeout: config.injection_timeout,
        block_threshold: config.injection_block_threshold,
        warn_threshold: config.injection_warn_threshold,
        http: http.clone(),
    });
    info!(
        kind = "startup.classifier_ready",
        ollama_url = %config.ollama_url,
        ollama_model = %config.ollama_model,
        block_threshold = config.injection_block_threshold,
        warn_threshold = config.injection_warn_threshold,
        timeout_secs = config.injection_timeout.as_secs(),
        "classifier ready"
    );

    let state = SharedState {
        slack: slack_client.clone(),
        config: config.clone(),
        http,
        self_bot_id,
        self_user_id,
        classifier,
    };
    SHARED
        .set(state.clone())
        .map_err(|_| "shared state already set")?;

    let http_state = state.clone();
    let http_task = tokio::spawn(async move { run_http_server(http_state).await });

    let callbacks = SlackSocketModeListenerCallbacks::new().with_push_events(handle_push_event);
    let env = Arc::new(
        SlackClientEventsListenerEnvironment::new(slack_client.clone())
            .with_error_handler(on_error),
    );
    let listener =
        SlackClientSocketModeListener::new(&SlackClientSocketModeConfig::new(), env, callbacks);

    info!(
        kind = "startup.socket_mode_connecting",
        label = %config.salon_label,
        target = %config.salon_target,
        "connecting via socket mode"
    );
    listener.listen_for(&config.app_token).await?;

    tokio::select! {
        _ = listener.serve() => {
            warn!(
                kind = "internal.listener_ended",
                "slack socket-mode listener returned"
            );
        }
        res = http_task => {
            warn!(
                kind = "internal.http_task_ended",
                error = %format!("{res:?}"),
                "http server task ended"
            );
        }
    }
    Ok(())
}
