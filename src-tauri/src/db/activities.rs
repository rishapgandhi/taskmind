//! Activities, projects and project rules.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Db, DbError};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRow {
    pub id: i64,
    pub started_at: String,
    pub ended_at: String,
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub project_id: Option<i64>,
    pub is_idle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuleRow {
    pub kind: String,
    pub pattern: String,
    #[serde(default)]
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub rules: Vec<RuleRow>,
}

pub const RULE_KINDS: &[&str] = &["window_title", "app_name", "git_branch", "directory", "url"];

pub struct NewActivity<'a> {
    pub started_at: &'a str,
    pub ended_at: &'a str,
    pub app_name: &'a str,
    pub window_title: &'a str,
    pub url: Option<&'a str>,
    pub project_id: Option<i64>,
    pub is_idle: bool,
}

impl Db {
    pub fn insert_activity(&self, a: &NewActivity<'_>) -> Result<i64, DbError> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO activities (started_at, ended_at, app_name, window_title, url, project_id, is_idle)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![a.started_at, a.ended_at, a.app_name, a.window_title, a.url, a.project_id, a.is_idle],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn extend_activity(&self, id: i64, ended_at: &str) -> Result<(), DbError> {
        self.lock()?.execute(
            "UPDATE activities SET ended_at = ?2 WHERE id = ?1",
            params![id, ended_at],
        )?;
        Ok(())
    }

    /// Activities overlapping [from, to) (UTC ISO strings), oldest first.
    pub fn activities_between(&self, from: &str, to: &str) -> Result<Vec<ActivityRow>, DbError> {
        let conn = self.lock()?;
        let rows = conn
            .prepare(
                "SELECT id, started_at, ended_at, COALESCE(app_name, ''), COALESCE(window_title, ''),
                        url, project_id, is_idle
                 FROM activities WHERE ended_at > ?1 AND started_at < ?2 ORDER BY started_at",
            )?
            .query_map(params![from, to], |r| {
                Ok(ActivityRow {
                    id: r.get(0)?,
                    started_at: r.get(1)?,
                    ended_at: r.get(2)?,
                    app_name: r.get(3)?,
                    window_title: r.get(4)?,
                    url: r.get(5)?,
                    project_id: r.get(6)?,
                    is_idle: r.get(7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn delete_activities_between(&self, from: &str, to: &str) -> Result<usize, DbError> {
        Ok(self.lock()?.execute(
            "DELETE FROM activities WHERE started_at >= ?1 AND started_at < ?2",
            params![from, to],
        )?)
    }

    pub fn projects(&self) -> Result<Vec<ProjectRow>, DbError> {
        let conn = self.lock()?;
        let mut projects: Vec<ProjectRow> = conn
            .prepare("SELECT id, name, color FROM projects ORDER BY name COLLATE NOCASE")?
            .query_map([], |r| {
                Ok(ProjectRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    color: r.get(2)?,
                    rules: Vec::new(),
                })
            })?
            .collect::<Result<_, _>>()?;
        let mut stmt = conn.prepare(
            "SELECT project_id, kind, pattern, priority FROM project_rules ORDER BY priority DESC, id",
        )?;
        let rules = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                RuleRow {
                    kind: r.get(1)?,
                    pattern: r.get(2)?,
                    priority: r.get(3)?,
                },
            ))
        })?;
        for rule in rules {
            let (pid, rule) = rule?;
            if let Some(p) = projects.iter_mut().find(|p| p.id == pid) {
                p.rules.push(rule);
            }
        }
        Ok(projects)
    }

    /// Create (id = None) or update a project and replace its rules.
    pub fn save_project(
        &self,
        id: Option<i64>,
        name: &str,
        color: Option<&str>,
        rules: &[RuleRow],
        now: &str,
    ) -> Result<i64, DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let id = match id {
            Some(id) => {
                tx.execute(
                    "UPDATE projects SET name = ?2, color = ?3 WHERE id = ?1",
                    params![id, name, color],
                )?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO projects (name, color, created_at) VALUES (?1, ?2, ?3)",
                    params![name, color, now],
                )?;
                tx.last_insert_rowid()
            }
        };
        tx.execute(
            "DELETE FROM project_rules WHERE project_id = ?1",
            params![id],
        )?;
        for r in rules {
            tx.execute(
                "INSERT INTO project_rules (project_id, kind, pattern, priority) VALUES (?1, ?2, ?3, ?4)",
                params![id, r.kind, r.pattern, r.priority],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn delete_project(&self, id: i64) -> Result<(), DbError> {
        self.lock()?
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn project_by_name(&self, name: &str) -> Result<Option<i64>, DbError> {
        Ok(self
            .lock()?
            .query_row(
                "SELECT id FROM projects WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Re-run the classifier over activities since `from`.
    pub fn reclassify_since(
        &self,
        from: &str,
        classify: impl Fn(&str, &str, Option<&str>) -> Option<i64>,
    ) -> Result<usize, DbError> {
        let mut conn = self.lock()?;
        let tx = conn.transaction()?;
        let rows: Vec<(i64, String, String, Option<String>)> = tx
            .prepare(
                "SELECT id, COALESCE(app_name, ''), COALESCE(window_title, ''), url
                 FROM activities WHERE started_at >= ?1 AND is_idle = 0",
            )?
            .query_map(params![from], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<Result<_, _>>()?;
        for (id, app, title, url) in &rows {
            tx.execute(
                "UPDATE activities SET project_id = ?2 WHERE id = ?1",
                params![id, classify(app, title, url.as_deref())],
            )?;
        }
        tx.commit()?;
        Ok(rows.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_and_activities() {
        let db = Db::open_in_memory().unwrap();
        let rules = vec![RuleRow {
            kind: "window_title".into(),
            pattern: "permit".into(),
            priority: 0,
        }];
        let id = db
            .save_project(None, "Permit Vault", Some("#4f46e5"), &rules, "now")
            .unwrap();
        db.save_project(Some(id), "Permit Vault", None, &rules, "now")
            .unwrap();
        let p = db.projects().unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].rules, rules);
        assert_eq!(db.project_by_name("permit vault").unwrap(), Some(id));

        let a = db
            .insert_activity(&NewActivity {
                started_at: "2026-11-27T10:00:00Z",
                ended_at: "2026-11-27T10:00:10Z",
                app_name: "Code",
                window_title: "permit-api — main.rs",
                url: None,
                project_id: None,
                is_idle: false,
            })
            .unwrap();
        db.extend_activity(a, "2026-11-27T10:05:00Z").unwrap();
        let rows = db
            .activities_between("2026-11-27T00:00:00Z", "2026-11-28T00:00:00Z")
            .unwrap();
        assert_eq!(rows[0].ended_at, "2026-11-27T10:05:00Z");

        let n = db
            .reclassify_since("2026-11-27T00:00:00Z", |_, t, _| {
                t.contains("permit").then_some(id)
            })
            .unwrap();
        assert_eq!(n, 1);
        let rows = db
            .activities_between("2026-11-27T00:00:00Z", "2026-11-28T00:00:00Z")
            .unwrap();
        assert_eq!(rows[0].project_id, Some(id));

        // Deleting a project keeps activities but unassigns them.
        db.delete_project(id).unwrap();
        let rows = db
            .activities_between("2026-11-27T00:00:00Z", "2026-11-28T00:00:00Z")
            .unwrap();
        assert_eq!(rows[0].project_id, None);
    }
}
