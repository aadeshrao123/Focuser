//! Tauri commands — called from the frontend, talk directly to the embedded engine.

use std::sync::Arc;
use focuser_common::types::*;
use serde_json::Value;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub fn get_status(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let lists = eng.block_lists();
    let active: Vec<Value> = lists
        .iter()
        .filter(|l| l.enabled)
        .map(|l| {
            serde_json::json!({
                "block_list_id": l.id.to_string(),
                "block_list_name": l.name,
                "started_at": l.created_at.to_rfc3339(),
                "expires_at": null,
                "blocked_websites": l.websites.len(),
                "blocked_apps": l.applications.len(),
            })
        })
        .collect();
    let total_blocked = eng.db().get_total_blocked_today().unwrap_or(0);
    Ok(serde_json::json!({
        "running": true,
        "active_blocks": active,
        "total_blocked_today": total_blocked,
        "uptime_seconds": 0,
    }))
}

#[tauri::command]
pub fn list_block_lists(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    serde_json::to_value(eng.block_lists()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_block_list(state: State<'_, Arc<AppState>>, name: String) -> Result<Value, String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let list = BlockList::new(&name);
    let id = list.id.to_string();
    eng.db().create_block_list(&list).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "id": id }))
}

#[tauri::command]
pub fn update_block_list(state: State<'_, Arc<AppState>>, list_json: String) -> Result<(), String> {
    let list: BlockList = serde_json::from_str(&list_json).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db().update_block_list(&list).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_block_list(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db().delete_block_list(uuid).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn toggle_block_list(state: State<'_, Arc<AppState>>, id: String, enabled: bool) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    list.enabled = enabled;
    list.updated_at = chrono::Utc::now();
    eng.db().update_block_list(&list).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn add_website_rule(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    rule_type: String,
    value: String,
) -> Result<Value, String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;

    let rule = match rule_type.as_str() {
        "domain" => WebsiteRule::domain(&value),
        "keyword" => WebsiteRule::keyword(&value),
        "wildcard" => WebsiteRule::wildcard(&value),
        "url_path" => WebsiteRule::url_path(&value),
        _ => return Err(format!("Unknown rule type: {rule_type}")),
    };
    let rule_id = rule.id.to_string();
    list.websites.push(rule);
    list.updated_at = chrono::Utc::now();
    eng.db().update_block_list(&list).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "id": rule_id }))
}

#[tauri::command]
pub fn remove_website_rule(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    rule_id: String,
) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    list.websites.retain(|r| r.id.to_string() != rule_id);
    list.updated_at = chrono::Utc::now();
    eng.db().update_block_list(&list).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn add_app_rule(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    rule_type: String,
    value: String,
) -> Result<Value, String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;

    let rule = match rule_type.as_str() {
        "exe_name" => AppRule::executable(&value),
        "exe_path" => AppRule::path(&value),
        "window_title" => AppRule::window_title(&value),
        _ => return Err(format!("Unknown rule type: {rule_type}")),
    };
    let rule_id = rule.id.to_string();
    list.applications.push(rule);
    list.updated_at = chrono::Utc::now();
    eng.db().update_block_list(&list).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "id": rule_id }))
}

#[tauri::command]
pub fn remove_app_rule(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    rule_id: String,
) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    list.applications.retain(|r| r.id.to_string() != rule_id);
    list.updated_at = chrono::Utc::now();
    eng.db().update_block_list(&list).map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn check_domain(state: State<'_, Arc<AppState>>, domain: String) -> Result<bool, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(eng.check_domain(&domain).is_some())
}

#[tauri::command]
pub fn get_stats(state: State<'_, Arc<AppState>>, from: String, to: String) -> Result<Value, String> {
    let from_date = chrono::NaiveDate::parse_from_str(&from, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date: {e}"))?;
    let to_date = chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date: {e}"))?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let stats = eng.db().get_stats(from_date, to_date).map_err(|e| e.to_string())?;
    serde_json::to_value(stats).map_err(|e| e.to_string())
}

/// Apply all current blocks to the hosts file (requires admin).
#[tauri::command]
pub fn apply_blocks(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let domains = eng.collect_blocked_domains();
    if domains.is_empty() {
        return Ok("No domains to block".into());
    }
    crate::blocker::apply_hosts_blocks(&domains).map_err(|e| e.to_string())?;
    Ok(format!("Blocked {} domains", domains.len()))
}

/// Remove all Focuser blocks from the hosts file.
#[tauri::command]
pub fn remove_blocks() -> Result<String, String> {
    crate::blocker::remove_hosts_blocks().map_err(|e| e.to_string())?;
    Ok("All blocks removed".into())
}
