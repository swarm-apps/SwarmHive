//! First-boot seeders for mail-related rows:
//! - 4 events × 2 locales = 8 default templates
//! - optional dev mailpit provider when `config.mail.seed_mailpit_in_dev = true`
//!
//! Both are idempotent: `seed_default_templates` only inserts rows whose
//! (event, locale) pair is missing; `seed_mailpit_provider` only runs when
//! the `mail_provider` table is empty.

use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};
use swarmhive_entity::{mail_provider, mail_template};
use tracing::info;
use uuid::Uuid;

/// `(event_name, locale, subject, html_body, text_body)`. Files embedded
/// from `crates/swarmhive-server/assets/mail-templates/<event>.<locale>.*`.
struct DefaultTemplate {
    event_name: &'static str,
    locale: &'static str,
    subject: &'static str,
    html_body: &'static str,
    text_body: &'static str,
}

const DEFAULT_TEMPLATES: &[DefaultTemplate] = &[
    DefaultTemplate {
        event_name: "user_invite",
        locale: "en",
        subject: include_str!("../../assets/mail-templates/user_invite.en.subject"),
        html_body: include_str!("../../assets/mail-templates/user_invite.en.html"),
        text_body: include_str!("../../assets/mail-templates/user_invite.en.text"),
    },
    DefaultTemplate {
        event_name: "user_invite",
        locale: "zh-CN",
        subject: include_str!("../../assets/mail-templates/user_invite.zh-CN.subject"),
        html_body: include_str!("../../assets/mail-templates/user_invite.zh-CN.html"),
        text_body: include_str!("../../assets/mail-templates/user_invite.zh-CN.text"),
    },
    DefaultTemplate {
        event_name: "password_reset",
        locale: "en",
        subject: include_str!("../../assets/mail-templates/password_reset.en.subject"),
        html_body: include_str!("../../assets/mail-templates/password_reset.en.html"),
        text_body: include_str!("../../assets/mail-templates/password_reset.en.text"),
    },
    DefaultTemplate {
        event_name: "password_reset",
        locale: "zh-CN",
        subject: include_str!("../../assets/mail-templates/password_reset.zh-CN.subject"),
        html_body: include_str!("../../assets/mail-templates/password_reset.zh-CN.html"),
        text_body: include_str!("../../assets/mail-templates/password_reset.zh-CN.text"),
    },
    DefaultTemplate {
        event_name: "email_verify",
        locale: "en",
        subject: include_str!("../../assets/mail-templates/email_verify.en.subject"),
        html_body: include_str!("../../assets/mail-templates/email_verify.en.html"),
        text_body: include_str!("../../assets/mail-templates/email_verify.en.text"),
    },
    DefaultTemplate {
        event_name: "email_verify",
        locale: "zh-CN",
        subject: include_str!("../../assets/mail-templates/email_verify.zh-CN.subject"),
        html_body: include_str!("../../assets/mail-templates/email_verify.zh-CN.html"),
        text_body: include_str!("../../assets/mail-templates/email_verify.zh-CN.text"),
    },
    DefaultTemplate {
        event_name: "security_alert",
        locale: "en",
        subject: include_str!("../../assets/mail-templates/security_alert.en.subject"),
        html_body: include_str!("../../assets/mail-templates/security_alert.en.html"),
        text_body: include_str!("../../assets/mail-templates/security_alert.en.text"),
    },
    DefaultTemplate {
        event_name: "security_alert",
        locale: "zh-CN",
        subject: include_str!("../../assets/mail-templates/security_alert.zh-CN.subject"),
        html_body: include_str!("../../assets/mail-templates/security_alert.zh-CN.html"),
        text_body: include_str!("../../assets/mail-templates/security_alert.zh-CN.text"),
    },
];

/// Idempotent: inserts any of the 8 default rows that are missing. Safe to
/// run on every startup.
pub async fn seed_default_templates(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    let mut inserted = 0;
    for tpl in DEFAULT_TEMPLATES {
        let existing = mail_template::Entity::find()
            .filter(mail_template::Column::EventName.eq(tpl.event_name))
            .filter(mail_template::Column::Locale.eq(tpl.locale))
            .count(db)
            .await?;
        if existing > 0 {
            continue;
        }
        mail_template::ActiveModel {
            id: Set(Uuid::now_v7()),
            event_name: Set(tpl.event_name.into()),
            locale: Set(tpl.locale.into()),
            subject: Set(tpl.subject.trim().to_string()),
            html_body: Set(tpl.html_body.to_string()),
            text_body: Set(tpl.text_body.to_string()),
            updated_at: NotSet,
        }
        .insert(db)
        .await?;
        inserted += 1;
    }
    if inserted > 0 {
        info!(inserted, "default mail templates seeded");
    }
    Ok(())
}

/// Force-restore every default template by overwriting the matching row's
/// subject / body. Called by `POST /api/v1/mail/templates/seed-defaults`.
pub async fn restore_default_templates(db: &DatabaseConnection) -> Result<usize, sea_orm::DbErr> {
    let mut touched = 0;
    for tpl in DEFAULT_TEMPLATES {
        let existing = mail_template::Entity::find()
            .filter(mail_template::Column::EventName.eq(tpl.event_name))
            .filter(mail_template::Column::Locale.eq(tpl.locale))
            .one(db)
            .await?;
        let mut am = match existing {
            Some(row) => row.into(),
            None => mail_template::ActiveModel {
                id: Set(Uuid::now_v7()),
                event_name: Set(tpl.event_name.into()),
                locale: Set(tpl.locale.into()),
                updated_at: NotSet,
                subject: NotSet,
                html_body: NotSet,
                text_body: NotSet,
            },
        };
        am.subject = Set(tpl.subject.trim().to_string());
        am.html_body = Set(tpl.html_body.to_string());
        am.text_body = Set(tpl.text_body.to_string());
        am.save(db).await?;
        touched += 1;
    }
    Ok(touched)
}

/// Dev-only: seed a single active provider pointing at mailpit
/// (`localhost:1025`, no auth). Runs only when the `mail_provider` table is
/// empty AND `config.mail.seed_mailpit_in_dev = true`.
pub async fn seed_mailpit_provider(db: &DatabaseConnection) -> Result<bool, sea_orm::DbErr> {
    let count = mail_provider::Entity::find().count(db).await?;
    if count > 0 {
        return Ok(false);
    }
    mail_provider::ActiveModel {
        id: Set(Uuid::now_v7()),
        name: Set("Mailpit (dev)".into()),
        kind: Set(mail_provider::ProviderKind::Smtp),
        active: Set(true),
        host: Set("localhost".into()),
        port: Set(1025),
        username: Set(None),
        password_encrypted: Set(None),
        encryption: Set(mail_provider::SmtpEncryption::None),
        from_email: Set("noreply@swarmhive.local".into()),
        from_name: Set(Some("SwarmHive (dev)".into())),
        reply_to: Set(None),
        created_at: NotSet,
        updated_at: NotSet,
    }
    .insert(db)
    .await?;
    info!("mailpit dev provider seeded (localhost:1025)");
    Ok(true)
}
