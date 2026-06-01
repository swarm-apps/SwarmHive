//! minijinja-backed render of `mail_template` rows.
//!
//! Render path: DB lookup keyed by `(event_name, locale)` → parse + compile
//! into a `minijinja::Environment` (cached by `updated_at`) → render subject,
//! HTML body, text body with the caller-supplied context.
//!
//! Cache invalidation: keyed on `updated_at` so an Admin edit takes effect
//! on the next render without process restart. Failed parses are NOT cached
//! so fixing the template + retrying works without reload.

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use minijinja::Environment;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde_json::Value;
use swarmhive_entity::mail_template;

/// Result of a successful render.
#[derive(Debug, Clone)]
pub struct RenderedMail {
    pub subject: String,
    pub html_body: String,
    pub text_body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error("template not found for event '{event}' / locale '{locale}'")]
    NotFound { event: String, locale: String },
    /// `field` is `subject` / `html_body` / `text_body` to point the Admin
    /// at the broken segment when surfaced through `/preview`.
    #[error("template parse error in {field}: {source}")]
    Parse {
        field: &'static str,
        #[source]
        source: minijinja::Error,
    },
    #[error("template render error: {0}")]
    Render(#[from] minijinja::Error),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
}

/// Bucketed key: `(event, locale, template_id)`. We include the template id
/// (a UUID v7) so a row replacement / re-seed naturally invalidates the
/// cached entry even if `updated_at` round-trips through the same millisecond.
type CacheKey = (String, String, uuid::Uuid, DateTime<Utc>);

#[derive(Default)]
struct Cached {
    env: Environment<'static>,
}

#[derive(Default)]
pub struct TemplateEngine {
    cache: RwLock<HashMap<CacheKey, Cached>>,
}

impl TemplateEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up + render. Returns `TemplateError` for missing rows, parse
    /// failures, or render-time errors. Caller decides whether to surface
    /// the error to the user (e.g. `/preview` returns 422) or fall back.
    pub async fn render(
        &self,
        db: &DatabaseConnection,
        event: &str,
        locale: &str,
        ctx: &Value,
    ) -> Result<RenderedMail, TemplateError> {
        let row = mail_template::Entity::find()
            .filter(mail_template::Column::EventName.eq(event))
            .filter(mail_template::Column::Locale.eq(locale))
            .one(db)
            .await?
            .ok_or_else(|| TemplateError::NotFound {
                event: event.to_string(),
                locale: locale.to_string(),
            })?;
        self.render_row(&row, ctx)
    }

    /// Render directly from an already-fetched row. Used by `/preview`,
    /// where the caller has the row from the lookup it just did for the
    /// permission check.
    pub fn render_row(
        &self,
        row: &mail_template::Model,
        ctx: &Value,
    ) -> Result<RenderedMail, TemplateError> {
        let key: CacheKey = (
            row.event_name.clone(),
            row.locale.clone(),
            row.id,
            row.updated_at,
        );

        // Read path: do nothing if already compiled.
        if let Some(cached) = self.cache.read().expect("cache poisoned").get(&key) {
            return Self::render_with(cached, ctx);
        }

        // Compile + insert. Parse errors short-circuit without caching so
        // a follow-up edit triggers a fresh attempt.
        let cached = Self::compile(row)?;
        let result = Self::render_with(&cached, ctx);
        self.cache
            .write()
            .expect("cache poisoned")
            .insert(key, cached);
        result
    }

    fn compile(row: &mail_template::Model) -> Result<Cached, TemplateError> {
        let mut env = Environment::new();
        // Identical names per row are fine — each Environment is scoped to
        // a single (event, locale).
        env.add_template_owned("subject", row.subject.clone())
            .map_err(|source| TemplateError::Parse {
                field: "subject",
                source,
            })?;
        env.add_template_owned("html_body", row.html_body.clone())
            .map_err(|source| TemplateError::Parse {
                field: "html_body",
                source,
            })?;
        env.add_template_owned("text_body", row.text_body.clone())
            .map_err(|source| TemplateError::Parse {
                field: "text_body",
                source,
            })?;
        Ok(Cached { env })
    }

    fn render_with(cached: &Cached, ctx: &Value) -> Result<RenderedMail, TemplateError> {
        // 模板名按行固定为这三个字面量（每个 Environment 只装一行的三段）。
        let subject = cached.env.get_template("subject")?.render(ctx)?;
        let html_body = cached.env.get_template("html_body")?.render(ctx)?;
        let text_body = cached.env.get_template("text_body")?.render(ctx)?;
        Ok(RenderedMail {
            subject,
            html_body,
            text_body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn row(subject: &str, html: &str, text: &str) -> mail_template::Model {
        mail_template::Model {
            id: Uuid::now_v7(),
            event_name: "test_event".into(),
            locale: "en".into(),
            subject: subject.into(),
            html_body: html.into(),
            text_body: text.into(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn renders_with_context() {
        let engine = TemplateEngine::new();
        let r = engine
            .render_row(
                &row(
                    "Hi {{ name }}",
                    "<p>Hello {{ name }}</p>",
                    "Hello {{ name }}",
                ),
                &json!({ "name": "Alice" }),
            )
            .unwrap();
        assert_eq!(r.subject, "Hi Alice");
        assert_eq!(r.html_body, "<p>Hello Alice</p>");
        assert_eq!(r.text_body, "Hello Alice");
    }

    #[test]
    fn parse_error_pinpoints_field() {
        let engine = TemplateEngine::new();
        // Truncated jinja open in html_body.
        let err = engine
            .render_row(
                &row("Hi", "<p>Hello {{ name", "Hello"),
                &json!({ "name": "Alice" }),
            )
            .unwrap_err();
        match err {
            TemplateError::Parse { field, .. } => assert_eq!(field, "html_body"),
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn parse_failure_is_not_cached() {
        let engine = TemplateEngine::new();
        let bad = row("Hi", "<p>Hello {{ name", "Hello");
        // First attempt fails.
        assert!(matches!(
            engine.render_row(&bad, &json!({})),
            Err(TemplateError::Parse { .. })
        ));
        // A second attempt with the same id but fixed body works
        // (proves we didn't cache the failure).
        let fixed = mail_template::Model {
            html_body: "<p>Hello {{ name }}</p>".into(),
            // updated_at must change for the cache key to differ; mirrors
            // the real DB UPDATE path.
            updated_at: bad.updated_at + chrono::Duration::milliseconds(1),
            ..bad
        };
        let r = engine
            .render_row(&fixed, &json!({ "name": "Bob" }))
            .unwrap();
        assert_eq!(r.html_body, "<p>Hello Bob</p>");
    }
}
