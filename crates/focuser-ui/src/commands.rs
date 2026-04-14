//! Tauri commands — called from the frontend, talk directly to the embedded engine.

use focuser_common::types::WebsiteMatchType;
use focuser_common::types::*;
use serde_json::Value;
use std::sync::Arc;
use tauri::State;
use tauri_plugin_updater::UpdaterExt;

use crate::AppState;

fn sync_hosts_now(eng: &focuser_core::BlockEngine) {
    sync_hosts_now_static(eng);
}

pub fn sync_hosts_now_static(eng: &focuser_core::BlockEngine) {
    let domains = eng.collect_blocked_domains();
    let _ = crate::blocker::apply_hosts_blocks(&domains);
}

fn check_protected(eng: &focuser_core::BlockEngine, id: uuid::Uuid) -> Result<(), String> {
    if eng.is_block_list_protected(id) {
        Err("Protection is active — cannot modify this block list until it expires".to_string())
    } else {
        Ok(())
    }
}

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
    eng.db()
        .create_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "id": id }))
}

#[tauri::command]
pub fn update_block_list(state: State<'_, Arc<AppState>>, list_json: String) -> Result<(), String> {
    let list: BlockList = serde_json::from_str(&list_json).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    if eng.is_block_list_protected(list.id) {
        return Err("Protection is active — cannot modify this block list".to_string());
    }
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(())
}

#[tauri::command]
pub fn delete_block_list(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    if eng.is_block_list_protected(uuid) {
        return Err("Protection is active — cannot delete this block list".to_string());
    }
    eng.db()
        .delete_block_list(uuid)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(())
}

#[tauri::command]
pub fn toggle_block_list(
    state: State<'_, Arc<AppState>>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    if !enabled && eng.is_block_list_protected(uuid) {
        return Err("Protection is active — cannot disable this block list".to_string());
    }
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    list.enabled = enabled;
    list.updated_at = chrono::Utc::now();
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
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
    check_protected(&eng, uuid)?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;

    let rule = match rule_type.as_str() {
        "domain" => WebsiteRule::domain(&value),
        "keyword" => WebsiteRule::keyword(&value),
        "wildcard" => WebsiteRule::wildcard(&value),
        "url_path" => WebsiteRule::url_path(&value),
        "entire_internet" => WebsiteRule::entire_internet(),
        _ => return Err(format!("Unknown rule type: {rule_type}")),
    };
    let rule_id = rule.id.to_string();
    list.websites.push(rule);
    list.updated_at = chrono::Utc::now();
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
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
    check_protected(&eng, uuid)?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    list.websites.retain(|r| r.id.to_string() != rule_id);
    list.updated_at = chrono::Utc::now();
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
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
    check_protected(&eng, uuid)?;
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
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
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
    check_protected(&eng, uuid)?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    list.applications.retain(|r| r.id.to_string() != rule_id);
    list.updated_at = chrono::Utc::now();
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(())
}

#[tauri::command]
pub fn check_domain(state: State<'_, Arc<AppState>>, domain: String) -> Result<bool, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    Ok(eng.check_domain(&domain).is_some())
}

#[tauri::command]
pub fn get_stats(
    state: State<'_, Arc<AppState>>,
    from: String,
    to: String,
) -> Result<Value, String> {
    let from_date = chrono::NaiveDate::parse_from_str(&from, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date: {e}"))?;
    let to_date = chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date: {e}"))?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let stats = eng
        .db()
        .get_stats(from_date, to_date)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(stats).map_err(|e| e.to_string())
}

/// Get fine-grained blocked events for timeline charts.
#[tauri::command]
pub fn get_blocked_events(
    state: State<'_, Arc<AppState>>,
    from: String,
    to: String,
) -> Result<Value, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let events = eng
        .db()
        .get_blocked_events(&from, &to)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(events).map_err(|e| e.to_string())
}

/// Update the schedule for a block list.
/// If `always_active` is true, clears the schedule (list blocks at all times).
/// Otherwise, sets the schedule to the given time slots.
#[tauri::command]
pub fn update_schedule(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    slots: Vec<Value>,
    always_active: Option<bool>,
) -> Result<String, String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| format!("Invalid ID: {e}"))?;

    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    check_protected(&eng, uuid)?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;

    if always_active.unwrap_or(false) {
        // Always active — remove schedule entirely
        list.schedule = None;
    } else {
        // Convert JSON slots to TimeSlot structs
        let mut time_slots = Vec::new();
        for slot in &slots {
            let day_str = slot["day"].as_str().unwrap_or("");
            let hour = slot["hour"].as_u64().unwrap_or(0) as u32;

            let day = match day_str {
                "Mon" => chrono::Weekday::Mon,
                "Tue" => chrono::Weekday::Tue,
                "Wed" => chrono::Weekday::Wed,
                "Thu" => chrono::Weekday::Thu,
                "Fri" => chrono::Weekday::Fri,
                "Sat" => chrono::Weekday::Sat,
                "Sun" => chrono::Weekday::Sun,
                _ => continue,
            };

            let start = chrono::NaiveTime::from_hms_opt(hour, 0, 0).unwrap_or_default();
            let end = chrono::NaiveTime::from_hms_opt((hour + 1) % 24, 0, 0).unwrap_or_default();
            time_slots.push(focuser_common::types::TimeSlot { day, start, end });
        }

        list.schedule = Some(focuser_common::types::Schedule {
            id: uuid::Uuid::new_v4(),
            name: format!("{} schedule", list.name),
            time_slots,
            enabled: true,
        });
    }
    list.updated_at = chrono::Utc::now();

    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    let _ = eng.refresh();

    Ok("Schedule saved".into())
}

/// Apply all current blocks to the hosts file.
#[tauri::command]
pub fn apply_blocks(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let domains = eng.collect_blocked_domains();
    if domains.is_empty() {
        crate::blocker::remove_hosts_blocks().map_err(|e| e.to_string())?;
        return Ok("No domains to block — hosts file cleaned".into());
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

/// Bulk import domains into a block list.
#[tauri::command]
pub fn bulk_import_websites(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    domains: Vec<String>,
    rule_type: String,
) -> Result<Value, String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    check_protected(&eng, uuid)?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;

    let mut added = 0u32;
    for d in &domains {
        let trimmed = d.trim().to_lowercase();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Skip duplicates
        let already_exists = list.websites.iter().any(|r| match &r.match_type {
            WebsiteMatchType::Domain(existing) => existing.to_lowercase() == trimmed,
            WebsiteMatchType::Keyword(existing) => existing.to_lowercase() == trimmed,
            WebsiteMatchType::Wildcard(existing) => existing.to_lowercase() == trimmed,
            WebsiteMatchType::UrlPath(existing) => existing.to_lowercase() == trimmed,
            _ => false,
        });
        if already_exists {
            continue;
        }

        let rule = match rule_type.as_str() {
            "keyword" => WebsiteRule::keyword(&trimmed),
            "wildcard" => WebsiteRule::wildcard(&trimmed),
            "url_path" => WebsiteRule::url_path(&trimmed),
            _ => WebsiteRule::domain(&trimmed),
        };
        list.websites.push(rule);
        added += 1;
    }

    list.updated_at = chrono::Utc::now();
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(serde_json::json!({ "added": added }))
}

/// Add an exception to a block list.
#[tauri::command]
pub fn add_exception(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    domain: String,
    exception_type: String,
) -> Result<Value, String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    check_protected(&eng, uuid)?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;

    use focuser_common::types::ExceptionRule;
    let exc = match exception_type.as_str() {
        "wildcard" => ExceptionRule {
            id: focuser_common::types::new_id(),
            exception_type: focuser_common::types::ExceptionType::Wildcard(domain),
            enabled: true,
        },
        _ => ExceptionRule::domain(&domain),
    };
    let exc_id = exc.id.to_string();
    list.exceptions.push(exc);
    list.updated_at = chrono::Utc::now();
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(serde_json::json!({ "id": exc_id }))
}

/// Remove an exception from a block list.
#[tauri::command]
pub fn remove_exception(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    exception_id: String,
) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    check_protected(&eng, uuid)?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    list.exceptions.retain(|e| e.id.to_string() != exception_id);
    list.updated_at = chrono::Utc::now();
    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(())
}

/// Clear all website rules from all block lists.
#[tauri::command]
pub fn clear_all_websites(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let lists = eng.db().list_block_lists().map_err(|e| e.to_string())?;
    let mut cleared = 0u32;
    for mut list in lists {
        if !list.websites.is_empty() && !list.is_modification_protected() {
            cleared += list.websites.len() as u32;
            list.websites.clear();
            list.updated_at = chrono::Utc::now();
            eng.db()
                .update_block_list(&list)
                .map_err(|e| e.to_string())?;
        }
    }
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(serde_json::json!({ "cleared": cleared }))
}

/// Clear all app rules from all block lists.
#[tauri::command]
pub fn clear_all_apps(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let lists = eng.db().list_block_lists().map_err(|e| e.to_string())?;
    let mut cleared = 0u32;
    for mut list in lists {
        if !list.applications.is_empty() && !list.is_modification_protected() {
            cleared += list.applications.len() as u32;
            list.applications.clear();
            list.updated_at = chrono::Utc::now();
            eng.db()
                .update_block_list(&list)
                .map_err(|e| e.to_string())?;
        }
    }
    eng.refresh().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "cleared": cleared }))
}

/// Open a native file picker to select an application to block.
/// Works on Windows, macOS, and Linux — Tauri handles the platform-specific dialog.
#[tauri::command]
pub fn pick_app_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = std::sync::mpsc::channel();

    app.dialog()
        .file()
        .set_title("Select Application to Block")
        .add_filter("Executables", &["exe", "app", "sh", "AppImage"])
        .add_filter("All Files", &["*"])
        .pick_file(move |path| {
            let result = path.map(|p| {
                let path_str = p.to_string();
                std::path::Path::new(&path_str)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or(path_str)
            });
            let _ = tx.send(result);
        });

    rx.recv().map_err(|e| format!("Dialog error: {e}"))
}

#[tauri::command]
pub fn enable_protection(
    state: State<'_, Arc<AppState>>,
    list_id: String,
    duration_minutes: u32,
    prevent_uninstall: bool,
    prevent_service_stop: bool,
    prevent_modification: bool,
) -> Result<(), String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let mut list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;

    if list.is_modification_protected() {
        return Err("Protection is already active on this block list".to_string());
    }

    let now = chrono::Utc::now();
    list.protection = Some(focuser_common::types::Protection {
        prevent_uninstall,
        prevent_service_stop,
        prevent_modification,
        started_at: now,
        expires_at: now + chrono::Duration::minutes(duration_minutes as i64),
    });
    list.enabled = true;
    list.updated_at = now;

    eng.db()
        .update_block_list(&list)
        .map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(())
}

#[tauri::command]
pub fn get_protection_status(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let infos: Vec<Value> = eng
        .block_lists()
        .iter()
        .filter(|l| l.has_active_protection())
        .map(|l| {
            let p = l.protection.as_ref().unwrap();
            serde_json::json!({
                "block_list_id": l.id.to_string(),
                "block_list_name": l.name,
                "prevent_uninstall": p.prevent_uninstall,
                "prevent_service_stop": p.prevent_service_stop,
                "prevent_modification": p.prevent_modification,
                "remaining_seconds": p.remaining_seconds(),
                "expires_at": p.expires_at.to_rfc3339(),
            })
        })
        .collect();
    Ok(serde_json::json!(infos))
}

#[tauri::command]
pub fn export_block_list(
    state: State<'_, Arc<AppState>>,
    list_id: String,
) -> Result<String, String> {
    let uuid = uuid::Uuid::parse_str(&list_id).map_err(|e| e.to_string())?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let list = eng.db().get_block_list(uuid).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
}

// ─── Settings: Export/Import/Clear/Reset ─────────────────────────

/// Export the full Focuser configuration: all block lists, rules, schedules,
/// exceptions, and settings. Statistics are NOT exported.
/// Opens a save dialog so the user can choose where to write the file.
/// Returns the chosen path on success, or None if cancelled.
#[tauri::command]
pub fn export_configuration(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let json = {
        let eng = state.engine.lock().map_err(|e| e.to_string())?;
        let block_lists = eng.block_lists();
        let export = serde_json::json!({
            "version": 1,
            "exported_at": chrono::Utc::now().to_rfc3339(),
            "app": "Focuser",
            "block_lists": block_lists,
        });
        serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?
    };

    let default_name = format!(
        "focuser-config-{}.json",
        chrono::Local::now().format("%Y-%m-%d")
    );

    let (tx, rx) = std::sync::mpsc::channel();

    app.dialog()
        .file()
        .set_title("Export Focuser Configuration")
        .add_filter("JSON", &["json"])
        .set_file_name(&default_name)
        .save_file(move |path| {
            let _ = tx.send(path);
        });

    let chosen = rx.recv().map_err(|e| format!("Dialog error: {e}"))?;

    match chosen {
        Some(path) => {
            let path_str = path.to_string();
            std::fs::write(&path_str, &json).map_err(|e| format!("Write failed: {e}"))?;
            Ok(Some(path_str))
        }
        None => Ok(None),
    }
}

/// Open a file picker to choose a config file to import, and return its contents.
/// Returns None if the user cancelled.
#[tauri::command]
pub fn pick_import_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = std::sync::mpsc::channel();

    app.dialog()
        .file()
        .set_title("Import Focuser Configuration")
        .add_filter("JSON", &["json"])
        .pick_file(move |path| {
            let _ = tx.send(path);
        });

    let chosen = rx.recv().map_err(|e| format!("Dialog error: {e}"))?;

    match chosen {
        Some(path) => {
            let path_str = path.to_string();
            let contents =
                std::fs::read_to_string(&path_str).map_err(|e| format!("Read failed: {e}"))?;
            Ok(Some(contents))
        }
        None => Ok(None),
    }
}

/// Import a Focuser configuration. REPLACES all existing block lists,
/// rules, and schedules with the imported data. Statistics are preserved.
#[tauri::command]
pub fn import_configuration(
    state: State<'_, Arc<AppState>>,
    json: String,
) -> Result<serde_json::Value, String> {
    let data: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    let block_lists_val = data
        .get("block_lists")
        .ok_or_else(|| "Invalid file: missing 'block_lists'".to_string())?;

    let imported_lists: Vec<BlockList> =
        serde_json::from_value(block_lists_val.clone()).map_err(|e| e.to_string())?;

    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;

    // Refuse import if any existing list is protected (Focus Locked)
    for list in eng.block_lists() {
        if eng.is_block_list_protected(list.id) {
            return Err(format!(
                "Cannot import — '{}' is Focus Locked. Wait for the lock to expire.",
                list.name
            ));
        }
    }

    // Delete all existing block lists
    let existing_ids: Vec<uuid::Uuid> = eng.block_lists().iter().map(|l| l.id).collect();
    for id in existing_ids {
        let _ = eng.db().delete_block_list(id);
    }

    // Insert imported lists
    let count = imported_lists.len();
    for list in &imported_lists {
        eng.db()
            .create_block_list(list)
            .map_err(|e| e.to_string())?;
    }

    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);

    Ok(serde_json::json!({ "imported": count }))
}

/// Clear all statistics and blocked events. Block lists are preserved.
#[tauri::command]
pub fn clear_statistics(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db().clear_all_statistics().map_err(|e| e.to_string())
}

/// Get the current statistics retention period in days.
/// Default is 30 days if not set.
#[tauri::command]
pub fn get_stats_retention(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let value = eng
        .db()
        .get_setting_or_default("stats_retention_days", "30")
        .map_err(|e| e.to_string())?;
    value.parse::<u32>().map_err(|e| e.to_string())
}

/// Set the statistics retention period in days and immediately cleanup
/// any statistics older than the new limit. Returns the number of
/// old stat rows that were deleted.
#[tauri::command]
pub fn set_stats_retention(state: State<'_, Arc<AppState>>, days: u32) -> Result<u64, String> {
    if days == 0 {
        return Err("Retention period must be at least 1 day".to_string());
    }
    if days > 36500 {
        return Err("Retention period too large (max 36500 days)".to_string());
    }
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db()
        .set_setting("stats_retention_days", &days.to_string())
        .map_err(|e| e.to_string())?;
    let deleted = eng
        .db()
        .cleanup_old_statistics(days)
        .map_err(|e| e.to_string())?;
    Ok(deleted)
}

/// Reset all settings to defaults. Block lists and statistics are preserved.
/// This only affects things like autostart preference, notification settings, etc.
#[tauri::command]
pub fn reset_settings(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db().clear_settings().map_err(|e| e.to_string())
}

/// Get the connection status of all known browsers. For each browser:
/// - `running`: whether the browser process is currently running
/// - `extension_connected`: whether the Focuser extension has heartbeated recently
///
/// Used by the dashboard to show the user which browsers are protected.
#[tauri::command]
pub fn get_browser_status() -> Result<Value, String> {
    let connected = crate::api::get_connected_browsers(120);
    let running = detect_running_browsers();

    let statuses: Vec<Value> = focuser_common::browser::KNOWN_BROWSERS
        .iter()
        .map(|info| {
            let is_running = running.contains(&info.browser_type);
            let has_extension = connected.contains(&info.browser_type);
            serde_json::json!({
                "browser_type": format!("{:?}", info.browser_type),
                "display_name": info.display_name,
                "running": is_running,
                "extension_connected": has_extension,
            })
        })
        .collect();

    Ok(serde_json::json!({ "browsers": statuses }))
}

#[cfg(windows)]
fn detect_running_browsers() -> std::collections::HashSet<focuser_common::extension::BrowserType> {
    use std::collections::HashSet;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::*;

    let mut found = HashSet::new();
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(_) => return found,
        };
        let mut entry = PROCESSENTRY32 {
            dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
            ..Default::default()
        };
        if Process32First(snapshot, &mut entry).is_ok() {
            loop {
                let name: String = entry
                    .szExeFile
                    .iter()
                    .take_while(|&&c| c != 0)
                    .map(|&c| c as u8 as char)
                    .collect();
                if let Some(info) = focuser_common::browser::identify_browser(&name) {
                    found.insert(info.browser_type.clone());
                }
                if Process32Next(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    found
}

#[cfg(not(windows))]
fn detect_running_browsers() -> std::collections::HashSet<focuser_common::extension::BrowserType> {
    std::collections::HashSet::new()
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let updater = app.updater_builder().build().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(serde_json::json!({
            "available": true,
            "version": update.version,
            "body": update.body.unwrap_or_default(),
        })),
        Ok(None) => Ok(serde_json::json!({ "available": false })),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn do_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater_builder().build().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("No update available")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_browser_url(browser: String, url: String) -> Result<(), String> {
    let exe_path = resolve_browser_exe(&browser);
    std::process::Command::new(&exe_path)
        .arg(&url)
        .spawn()
        .map_err(|e| format!("Failed to open {browser} at {exe_path}: {e}"))?;
    Ok(())
}

/// Resolve a browser short name to its full executable path by querying the
/// Windows Registry `App Paths` key — the standard mechanism Windows uses to
/// locate installed applications regardless of install location.
#[cfg(windows)]
fn resolve_browser_exe(browser: &str) -> String {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, RegCloseKey, RegOpenKeyExW,
        RegQueryValueExW,
    };
    use windows::core::PCWSTR;

    let exe_name = match browser {
        "chrome" => "chrome.exe",
        "firefox" => "firefox.exe",
        "msedge" => "msedge.exe",
        "brave" => "brave.exe",
        "opera" => "opera.exe",
        other => return other.to_string(),
    };

    let sub_key = format!(
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{}",
        exe_name
    );
    let wide_key: Vec<u16> = sub_key.encode_utf16().chain(std::iter::once(0)).collect();

    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let mut hkey = HKEY::default();
        let status =
            unsafe { RegOpenKeyExW(root, PCWSTR(wide_key.as_ptr()), 0, KEY_READ, &mut hkey) };
        if status.is_err() {
            continue;
        }

        let mut buf = vec![0u8; 1024];
        let mut buf_len = buf.len() as u32;

        let status = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR::null(),
                None,
                None,
                Some(buf.as_mut_ptr()),
                Some(&mut buf_len),
            )
        };

        unsafe {
            let _ = RegCloseKey(hkey);
        }

        if status.is_ok() && buf_len > 2 {
            let wide_buf: Vec<u16> = buf[..buf_len as usize]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let len = wide_buf
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(wide_buf.len());
            let path = String::from_utf16_lossy(&wide_buf[..len]);
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                return path;
            }
        }
    }

    if let Some(path) = find_exe_in_user_dirs(exe_name) {
        return path;
    }

    exe_name.to_string()
}

#[cfg(windows)]
fn find_exe_in_user_dirs(exe_name: &str) -> Option<String> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    let candidates: Vec<std::path::PathBuf> = match exe_name {
        "opera.exe" => vec![
            [&local, "Programs", "Opera", "opera.exe"].iter().collect(),
            [&local, "Programs", "Opera GX", "opera.exe"]
                .iter()
                .collect(),
        ],
        "brave.exe" => vec![
            [
                &local,
                "BraveSoftware",
                "Brave-Browser",
                "Application",
                "brave.exe",
            ]
            .iter()
            .collect(),
        ],
        "chrome.exe" => vec![
            [&local, "Google", "Chrome", "Application", "chrome.exe"]
                .iter()
                .collect(),
        ],
        _ => vec![],
    };
    candidates
        .into_iter()
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(not(windows))]
fn resolve_browser_exe(browser: &str) -> String {
    browser.to_string()
}

/// Delete EVERYTHING: block lists, rules, schedules, exceptions, statistics,
/// blocked events, and settings. This is irreversible.
#[tauri::command]
pub fn delete_all_data(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;

    // Refuse if any block list is Focus Locked
    for list in eng.block_lists() {
        if eng.is_block_list_protected(list.id) {
            return Err(format!(
                "Cannot delete all data — '{}' is Focus Locked. Wait for the lock to expire.",
                list.name
            ));
        }
    }

    eng.db().delete_all_data().map_err(|e| e.to_string())?;
    eng.refresh().map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(())
}

// ────────────────────────────────────────────────────────────────────
// Pomodoro commands
// ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn pomodoro_get_status(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let status = focuser_core::pomodoro::build_status(eng.db()).map_err(|e| e.to_string())?;
    match status {
        Some(s) => Ok(serde_json::to_value(s).map_err(|e| e.to_string())?),
        None => Ok(serde_json::Value::Null),
    }
}

#[tauri::command]
pub fn pomodoro_start(
    state: State<'_, Arc<AppState>>,
    block_list_id: String,
    work_secs: u32,
    short_break_secs: u32,
    long_break_secs: u32,
    cycles_until_long_break: u32,
) -> Result<Value, String> {
    let bl_id = uuid::Uuid::parse_str(&block_list_id).map_err(|e| e.to_string())?;
    let config = focuser_common::pomodoro::PomodoroConfig {
        work_secs,
        short_break_secs,
        long_break_secs,
        cycles_until_long_break,
    };
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let session = focuser_core::pomodoro::start_session(&mut eng, bl_id, config)
        .map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(serde_json::json!({
        "session_id": session.id,
        "block_list_id": session.block_list_id,
        "started_at": session.started_at,
    }))
}

#[tauri::command]
pub fn pomodoro_pause(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    focuser_core::pomodoro::pause_session(&mut eng).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pomodoro_resume(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    focuser_core::pomodoro::resume_session(&mut eng).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pomodoro_skip(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let outcome = focuser_core::pomodoro::skip_phase(&mut eng).map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(serde_json::json!({ "advanced": outcome.is_some() }))
}

#[tauri::command]
pub fn pomodoro_stop(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    let mut eng = state.engine.lock().map_err(|e| e.to_string())?;
    let stopped = focuser_core::pomodoro::stop_session(&mut eng).map_err(|e| e.to_string())?;
    sync_hosts_now(&eng);
    Ok(stopped)
}

#[tauri::command]
pub fn pomodoro_drain_events(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let events = state.drain_pomodoro_events();
    let json: Vec<Value> = events
        .iter()
        .map(|e| match e {
            crate::PomodoroEvent::PhaseAdvanced { to, cycle } => {
                serde_json::json!({ "kind": "phase_advanced", "to": to, "cycle": cycle })
            }
            crate::PomodoroEvent::TamperDetected => {
                serde_json::json!({ "kind": "tamper_detected" })
            }
        })
        .collect();
    Ok(Value::Array(json))
}

// ────────────────────────────────────────────────────────────────────
// Allowance commands
// ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn allowance_list(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let list = eng
        .db()
        .list_allowance_statuses()
        .map_err(|e| e.to_string())?;
    serde_json::to_value(list).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn allowance_create(
    state: State<'_, Arc<AppState>>,
    kind: String,
    value: String,
    daily_limit_secs: u32,
    strict_mode: bool,
) -> Result<Value, String> {
    let target = match kind.as_str() {
        "domain" => focuser_common::allowance::AllowanceMatch::Domain(value.trim().to_string()),
        "app" => focuser_common::allowance::AllowanceMatch::AppExecutable(value.trim().to_string()),
        _ => return Err(format!("unknown allowance kind: {kind}")),
    };
    let a = focuser_common::allowance::Allowance::new(target, daily_limit_secs, strict_mode);
    a.validate()?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db().create_allowance(&a).map_err(|e| e.to_string())?;
    state
        .allowance_tracker
        .rebuild_from_db(eng.db())
        .map_err(|e| e.to_string())?;
    serde_json::to_value(a).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn allowance_update(
    state: State<'_, Arc<AppState>>,
    id: String,
    daily_limit_secs: u32,
    strict_mode: bool,
    enabled: bool,
) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let Some(mut a) = eng.db().get_allowance(id).map_err(|e| e.to_string())? else {
        return Err("allowance not found".into());
    };
    a.daily_limit_secs = daily_limit_secs;
    a.strict_mode = strict_mode;
    a.enabled = enabled;
    a.validate()?;
    eng.db().update_allowance(&a).map_err(|e| e.to_string())?;
    state
        .allowance_tracker
        .rebuild_from_db(eng.db())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn allowance_delete(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db().delete_allowance(id).map_err(|e| e.to_string())?;
    state
        .allowance_tracker
        .rebuild_from_db(eng.db())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn allowance_reset_today(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db()
        .reset_allowance_usage_today(id)
        .map_err(|e| e.to_string())?;
    state
        .allowance_tracker
        .rebuild_from_db(eng.db())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn allowance_drain_notifications(state: State<'_, Arc<AppState>>) -> Result<Value, String> {
    let n = state.allowance_tracker.take_notifications();
    serde_json::to_value(n).map_err(|e| e.to_string())
}

// ────────────────────────────────────────────────────────────────────
// Generic settings (used by Focus section)
// ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
    default: Option<String>,
) -> Result<String, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db()
        .get_setting_or_default(&key, default.as_deref().unwrap_or(""))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(
    state: State<'_, Arc<AppState>>,
    key: String,
    value: String,
) -> Result<(), String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    eng.db()
        .set_setting(&key, &value)
        .map_err(|e| e.to_string())
}

// ────────────────────────────────────────────────────────────────────
// Statistics: Pomodoro + Allowance history
// ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn pomodoro_history(state: State<'_, Arc<AppState>>, days: u32) -> Result<Value, String> {
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let rows = eng
        .db()
        .get_pomodoro_history(days)
        .map_err(|e| e.to_string())?;
    let json: Vec<Value> = rows
        .into_iter()
        .map(|(started, cycles, total_secs)| {
            serde_json::json!({
                "started_at": started.to_rfc3339(),
                "completed_cycles": cycles,
                "total_work_secs": total_secs,
            })
        })
        .collect();
    Ok(Value::Array(json))
}

#[tauri::command]
pub fn allowance_history(
    state: State<'_, Arc<AppState>>,
    id: String,
    days: u32,
) -> Result<Value, String> {
    let id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    let eng = state.engine.lock().map_err(|e| e.to_string())?;
    let rows = eng
        .db()
        .get_allowance_usage_history(id, days)
        .map_err(|e| e.to_string())?;
    let json: Vec<Value> = rows
        .into_iter()
        .map(|(date, secs)| serde_json::json!({ "date": date, "used_secs": secs }))
        .collect();
    Ok(Value::Array(json))
}
