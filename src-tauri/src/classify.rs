//! Rule-based project classification (ported from v1 `classifier.py`).
//! Matching is case-insensitive "contains". Order: rule priority, then rule kind
//! (url > window_title > git_branch > directory > app_name), then project order.

use crate::db::ProjectRow;

#[derive(Debug, Clone)]
pub struct Rule {
    project_id: i64,
    kind: Kind,
    pattern: String,
    priority: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Url,
    WindowTitle,
    GitBranch,
    Directory,
    AppName,
}

impl Kind {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "url" => Kind::Url,
            "window_title" => Kind::WindowTitle,
            "git_branch" => Kind::GitBranch,
            "directory" => Kind::Directory,
            "app_name" => Kind::AppName,
            _ => return None,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct Classifier {
    rules: Vec<Rule>,
}

impl Classifier {
    pub fn new(projects: &[ProjectRow]) -> Self {
        let mut rules: Vec<Rule> = projects
            .iter()
            .flat_map(|p| {
                p.rules.iter().filter_map(move |r| {
                    let pattern = r.pattern.trim().to_lowercase();
                    (!pattern.is_empty()).then_some(Rule {
                        project_id: p.id,
                        kind: Kind::parse(&r.kind)?,
                        pattern,
                        priority: r.priority,
                    })
                })
            })
            .collect();
        rules.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.kind.cmp(&b.kind)));
        Self { rules }
    }

    pub fn classify(&self, app_name: &str, window_title: &str, url: Option<&str>) -> Option<i64> {
        let app = app_name.to_lowercase();
        let title = window_title.to_lowercase();
        let url = url.unwrap_or_default().to_lowercase();
        self.rules
            .iter()
            .find(|r| match r.kind {
                Kind::Url => url.contains(&r.pattern),
                // v1: title rules also look at the URL.
                Kind::WindowTitle => title.contains(&r.pattern) || url.contains(&r.pattern),
                // Editors and terminals show branch and folder in the title.
                Kind::GitBranch | Kind::Directory => title.contains(&r.pattern),
                Kind::AppName => app.contains(&r.pattern),
            })
            .map(|r| r.project_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::RuleRow;

    fn project(id: i64, rules: &[(&str, &str, i64)]) -> ProjectRow {
        ProjectRow {
            id,
            name: format!("p{id}"),
            color: None,
            rules: rules
                .iter()
                .map(|(k, p, pr)| RuleRow {
                    kind: (*k).into(),
                    pattern: (*p).into(),
                    priority: *pr,
                })
                .collect(),
        }
    }

    #[test]
    fn matching_and_order() {
        let c = Classifier::new(&[
            project(1, &[("app_name", "slack", 0)]),
            project(2, &[("window_title", "PERMIT", 0)]),
            project(3, &[("url", "jira.example/esg", 0)]),
            project(
                4,
                &[
                    ("window_title", "standup", 10),
                    ("bogus", "x", 0),
                    ("app_name", "  ", 0),
                ],
            ),
        ]);
        assert_eq!(c.classify("Slack", "general", None), Some(1));
        assert_eq!(c.classify("Code", "permit-api — main.rs", None), Some(2));
        // URL rule beats title rule at equal priority.
        assert_eq!(
            c.classify("Chrome", "Permit board", Some("https://jira.example/esg/1")),
            Some(3)
        );
        // Higher priority wins over everything.
        assert_eq!(c.classify("Slack", "standup notes", None), Some(4));
        // Title rule also matches the URL (v1 behavior).
        assert_eq!(
            c.classify("Chrome", "Board", Some("https://x/permit")),
            Some(2)
        );
        assert_eq!(c.classify("Finder", "Downloads", None), None);
    }
}
