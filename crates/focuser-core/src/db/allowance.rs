use chrono::{DateTime, Local, Utc};
use focuser_common::allowance::{Allowance, AllowanceMatch, AllowanceStatus};
use focuser_common::error::{FocuserError, Result};
use focuser_common::types::EntityId;
use rusqlite::{OptionalExtension, params};

use super::Database;

impl Database {
    pub fn create_allowance(&self, a: &Allowance) -> Result<()> {
        let conn = self.conn_lock()?;
        conn.execute(
            "INSERT INTO allowances (
                id, match_type, match_value, daily_limit_secs, strict_mode, enabled, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                a.id.to_string(),
                a.target.kind_str(),
                a.target.value_str(),
                a.daily_limit_secs as i64,
                a.strict_mode as i64,
                a.enabled as i64,
                a.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn update_allowance(&self, a: &Allowance) -> Result<()> {
        let conn = self.conn_lock()?;
        let rows = conn
            .execute(
                "UPDATE allowances SET
                    match_type = ?1,
                    match_value = ?2,
                    daily_limit_secs = ?3,
                    strict_mode = ?4,
                    enabled = ?5
                 WHERE id = ?6",
                params![
                    a.target.kind_str(),
                    a.target.value_str(),
                    a.daily_limit_secs as i64,
                    a.strict_mode as i64,
                    a.enabled as i64,
                    a.id.to_string(),
                ],
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        if rows == 0 {
            return Err(FocuserError::Database(format!(
                "allowance {} not found",
                a.id
            )));
        }
        Ok(())
    }

    pub fn delete_allowance(&self, id: EntityId) -> Result<()> {
        let conn = self.conn_lock()?;
        conn.execute(
            "DELETE FROM allowance_usage WHERE allowance_id = ?1",
            params![id.to_string()],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        conn.execute(
            "DELETE FROM allowances WHERE id = ?1",
            params![id.to_string()],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn list_allowances(&self) -> Result<Vec<Allowance>> {
        let conn = self.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, match_type, match_value, daily_limit_secs,
                        strict_mode, enabled, created_at
                 FROM allowances
                 ORDER BY created_at ASC",
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_allowance)
            .map_err(|e| FocuserError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    pub fn get_allowance(&self, id: EntityId) -> Result<Option<Allowance>> {
        let conn = self.conn_lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, match_type, match_value, daily_limit_secs,
                        strict_mode, enabled, created_at
                 FROM allowances WHERE id = ?1",
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        stmt.query_row(params![id.to_string()], row_to_allowance)
            .optional()
            .map_err(|e| FocuserError::Database(e.to_string()))
    }

    /// Increment today's used_secs for an allowance. Upserts the row if needed.
    pub fn increment_allowance_usage(&self, id: EntityId, add_secs: u32) -> Result<u32> {
        let conn = self.conn_lock()?;
        let today = Local::now().date_naive().to_string();
        conn.execute(
            "INSERT INTO allowance_usage (allowance_id, usage_date, used_secs)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(allowance_id, usage_date)
             DO UPDATE SET used_secs = used_secs + ?3",
            params![id.to_string(), today, add_secs as i64],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        let used: i64 = conn
            .query_row(
                "SELECT used_secs FROM allowance_usage
                 WHERE allowance_id = ?1 AND usage_date = ?2",
                params![id.to_string(), today],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(used.max(0) as u32)
    }

    pub fn get_allowance_used_today(&self, id: EntityId) -> Result<u32> {
        let conn = self.conn_lock()?;
        let today = Local::now().date_naive().to_string();
        let used: i64 = conn
            .query_row(
                "SELECT used_secs FROM allowance_usage
                 WHERE allowance_id = ?1 AND usage_date = ?2",
                params![id.to_string(), today],
                |row| row.get(0),
            )
            .unwrap_or(0);
        Ok(used.max(0) as u32)
    }

    pub fn reset_allowance_usage_today(&self, id: EntityId) -> Result<()> {
        let conn = self.conn_lock()?;
        let today = Local::now().date_naive().to_string();
        conn.execute(
            "DELETE FROM allowance_usage
             WHERE allowance_id = ?1 AND usage_date = ?2",
            params![id.to_string(), today],
        )
        .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn list_allowance_statuses(&self) -> Result<Vec<AllowanceStatus>> {
        let all = self.list_allowances()?;
        let mut out = Vec::with_capacity(all.len());
        for a in all {
            let used = self.get_allowance_used_today(a.id)?;
            out.push(AllowanceStatus::new(a, used));
        }
        Ok(out)
    }

    /// Delete usage rows older than `keep_days`.
    pub fn cleanup_old_allowance_usage(&self, keep_days: u32) -> Result<u64> {
        let conn = self.conn_lock()?;
        let cutoff =
            (Local::now().date_naive() - chrono::Duration::days(keep_days as i64)).to_string();
        let deleted = conn
            .execute(
                "DELETE FROM allowance_usage WHERE usage_date < ?1",
                params![cutoff],
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        Ok(deleted as u64)
    }

    /// Per-day usage for one allowance over the last N days (for stats).
    pub fn get_allowance_usage_history(
        &self,
        id: EntityId,
        days: u32,
    ) -> Result<Vec<(String, u32)>> {
        let conn = self.conn_lock()?;
        let cutoff = (Local::now().date_naive() - chrono::Duration::days(days as i64)).to_string();
        let mut stmt = conn
            .prepare(
                "SELECT usage_date, used_secs FROM allowance_usage
                 WHERE allowance_id = ?1 AND usage_date >= ?2
                 ORDER BY usage_date ASC",
            )
            .map_err(|e| FocuserError::Database(e.to_string()))?;
        let rows = stmt
            .query_map(params![id.to_string(), cutoff], |row| {
                let date: String = row.get(0)?;
                let secs: i64 = row.get(1)?;
                Ok((date, secs.max(0) as u32))
            })
            .map_err(|e| FocuserError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }
}

fn row_to_allowance(row: &rusqlite::Row<'_>) -> rusqlite::Result<Allowance> {
    let id_str: String = row.get(0)?;
    let kind: String = row.get(1)?;
    let value: String = row.get(2)?;
    let limit: i64 = row.get(3)?;
    let strict: i64 = row.get(4)?;
    let enabled: i64 = row.get(5)?;
    let created_str: String = row.get(6)?;

    let id = uuid::Uuid::parse_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let target = match kind.as_str() {
        "domain" => AllowanceMatch::Domain(value),
        "app" => AllowanceMatch::AppExecutable(value),
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "unknown allowance kind: {other}"
                )),
            ));
        }
    };
    let created_at: DateTime<Utc> = DateTime::parse_from_rfc3339(&created_str)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(Allowance {
        id,
        target,
        daily_limit_secs: limit.max(0) as u32,
        strict_mode: strict != 0,
        enabled: enabled != 0,
        created_at,
    })
}
