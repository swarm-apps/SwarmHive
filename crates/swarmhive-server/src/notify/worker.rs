//! Postgres outbox worker for notifications.
//!
//! The MVP uses interval polling plus `FOR UPDATE SKIP LOCKED`. LISTEN/NOTIFY
//! can be layered on later without changing the delivery model.

use chrono::{Duration as ChronoDuration, Utc};
use sea_orm::ActiveValue::Set;
use sea_orm::sea_query::{LockBehavior, LockType};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};
use swarmhive_api_types as api;
use swarmhive_entity::{
    notification_delivery, notification_delivery_attempt, notification_outbox,
    notification_subscription, webhook_endpoint,
};
use tracing::{info, warn};
use uuid::Uuid;

use crate::crypto::SecretKey;
use crate::state::{AppState, MailerSlot};

use super::channel::{
    DeliveryFailure, DeliveryFailureKind, DeliveryOutcome, DeliveryRequest, DeliveryTarget,
    EmailChannel, NotificationChannel, WebhookChannel,
};

const DEFAULT_BATCH_SIZE: u64 = 50;
const DEFAULT_MAX_ATTEMPTS: i32 = 5;
/// endpoint 连续失败超过这么多天则自动停用(Stripe ~3 天量级)。
const AUTO_DISABLE_AFTER_DAYS: i64 = 3;

#[derive(Clone)]
pub struct Worker {
    db: DatabaseConnection,
    secret_key: SecretKey,
    email: EmailChannel,
    webhook: WebhookChannel,
    batch_size: u64,
    max_attempts: i32,
}

impl Worker {
    pub fn new(db: DatabaseConnection, secret_key: SecretKey, mailer: MailerSlot) -> Self {
        Self {
            db,
            secret_key,
            email: EmailChannel::new(mailer),
            webhook: WebhookChannel::new(),
            batch_size: DEFAULT_BATCH_SIZE,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
        }
    }

    pub async fn run_once(&self) -> Result<WorkerStats, DbErr> {
        let outbox = self.expand_outbox_batch().await?;
        let deliveries = self.deliver_due_batch().await?;
        Ok(WorkerStats {
            outbox_dispatched: outbox,
            deliveries_attempted: deliveries,
        })
    }

    async fn expand_outbox_batch(&self) -> Result<u64, DbErr> {
        let txn = self.db.begin().await?;
        let rows = notification_outbox::Entity::find()
            .filter(
                notification_outbox::Column::Status.eq(notification_outbox::OutboxStatus::Pending),
            )
            .order_by_asc(notification_outbox::Column::CreatedAt)
            .limit(self.batch_size)
            .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
            .all(&txn)
            .await?;

        let mut dispatched = 0;
        for row in rows {
            self.fanout_outbox_row(&txn, &row).await?;
            let mut am = row.into_active_model();
            am.status = Set(notification_outbox::OutboxStatus::Dispatched);
            am.dispatched_at = Set(Some(Utc::now()));
            am.update(&txn).await?;
            dispatched += 1;
        }
        txn.commit().await?;
        Ok(dispatched)
    }

    async fn fanout_outbox_row<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
        row: &notification_outbox::Model,
    ) -> Result<(), DbErr> {
        let subs = notification_subscription::Entity::find()
            .filter(notification_subscription::Column::EventType.eq(row.event_type))
            .filter(
                Condition::any()
                    .add(notification_subscription::Column::AppId.is_null())
                    .add(notification_subscription::Column::AppId.eq(row.app_id)),
            )
            .all(db)
            .await?;

        for sub in subs {
            notification_delivery::ActiveModel {
                id: Set(Uuid::now_v7()),
                event_id: Set(row.id),
                event_type: Set(row.event_type),
                subscription_id: Set(sub.id),
                channel_kind: Set(sub.channel_kind),
                webhook_endpoint_id: Set(sub.webhook_endpoint_id),
                status: Set(notification_delivery::DeliveryStatus::Pending),
                response_code: Set(None),
                attempt: Set(0),
                last_error: Set(None),
                next_retry_at: Set(None),
                // 尚未投递,请求 / 响应快照为空;首次投递时由 worker 写入。
                request_body: sea_orm::ActiveValue::NotSet,
                request_timestamp: sea_orm::ActiveValue::NotSet,
                request_signature: sea_orm::ActiveValue::NotSet,
                response_body: sea_orm::ActiveValue::NotSet,
                created_at: sea_orm::ActiveValue::NotSet,
                updated_at: sea_orm::ActiveValue::NotSet,
            }
            .insert(db)
            .await?;
        }
        Ok(())
    }

    async fn deliver_due_batch(&self) -> Result<u64, DbErr> {
        let txn = self.db.begin().await?;
        let now = Utc::now();
        let rows = notification_delivery::Entity::find()
            .filter(
                Condition::any()
                    .add(
                        notification_delivery::Column::Status
                            .eq(notification_delivery::DeliveryStatus::Pending),
                    )
                    .add(
                        Condition::all()
                            .add(
                                notification_delivery::Column::Status
                                    .eq(notification_delivery::DeliveryStatus::Failed),
                            )
                            .add(notification_delivery::Column::NextRetryAt.lte(now)),
                    ),
            )
            .order_by_asc(notification_delivery::Column::CreatedAt)
            .limit(self.batch_size)
            .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
            .all(&txn)
            .await?;

        let mut attempted = 0;
        for row in rows {
            attempted += 1;
            self.deliver_one(&txn, row).await?;
        }
        txn.commit().await?;
        Ok(attempted)
    }

    async fn deliver_one<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
        delivery: notification_delivery::Model,
    ) -> Result<(), DbErr> {
        let endpoint_id = webhook_endpoint_of(&delivery);

        let request = match self.delivery_request(db, &delivery).await {
            Ok(req) => req,
            Err(err) => {
                // 前置错误(如 endpoint 已禁用 / 密钥坏)标记失败,但不据此改 endpoint 健康
                // ——避免「已禁用」这类前置失败再触发自动停用逻辑。
                self.mark_failure(db, delivery, err).await?;
                return Ok(());
            }
        };

        let result = match delivery.channel_kind {
            notification_subscription::NotificationChannelKind::Email => {
                self.email.deliver(request).await
            }
            notification_subscription::NotificationChannelKind::Webhook => {
                self.webhook.deliver(request).await
            }
        };

        match result {
            Ok(outcome) => {
                self.mark_success(db, delivery, outcome).await?;
                if let Some(id) = endpoint_id {
                    self.update_endpoint_health(db, id, true).await?;
                }
            }
            Err(err) => {
                let became_dead = self.mark_failure(db, delivery, err).await?;
                if became_dead && let Some(id) = endpoint_id {
                    self.update_endpoint_health(db, id, false).await?;
                }
            }
        }
        Ok(())
    }

    async fn delivery_request<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
        delivery: &notification_delivery::Model,
    ) -> Result<DeliveryRequest, DeliveryFailure> {
        let outbox = notification_outbox::Entity::find_by_id(delivery.event_id)
            .one(db)
            .await
            .map_err(|err| DeliveryFailure::retryable(err.to_string()))?
            .ok_or_else(|| DeliveryFailure::permanent("notification outbox row is missing"))?;
        let subscription = notification_subscription::Entity::find_by_id(delivery.subscription_id)
            .one(db)
            .await
            .map_err(|err| DeliveryFailure::retryable(err.to_string()))?
            .ok_or_else(|| DeliveryFailure::permanent("notification subscription is missing"))?;

        let event = api::NotificationEvent {
            id: outbox.id,
            event_type: outbox.event_type.into(),
            source: "swarmhive".into(),
            time: outbox.created_at,
            data: outbox.payload,
        };

        let target = match delivery.channel_kind {
            notification_subscription::NotificationChannelKind::Email => {
                let to = subscription.email_to.ok_or_else(|| {
                    DeliveryFailure::permanent("email subscription has no recipient")
                })?;
                DeliveryTarget::Email { to }
            }
            notification_subscription::NotificationChannelKind::Webhook => {
                let endpoint_id = delivery
                    .webhook_endpoint_id
                    .or(subscription.webhook_endpoint_id)
                    .ok_or_else(|| {
                        DeliveryFailure::permanent("webhook subscription has no endpoint")
                    })?;
                let endpoint = webhook_endpoint::Entity::find_by_id(endpoint_id)
                    .one(db)
                    .await
                    .map_err(|err| DeliveryFailure::retryable(err.to_string()))?
                    .ok_or_else(|| DeliveryFailure::permanent("webhook endpoint is missing"))?;
                if endpoint.disabled {
                    return Err(DeliveryFailure::permanent("webhook endpoint is disabled"));
                }
                let secret = self
                    .secret_key
                    .decrypt(&endpoint.secret_encrypted)
                    .map_err(|err| {
                        DeliveryFailure::permanent(format!("webhook secret decrypt failed: {err}"))
                    })?;
                // 轮换宽限期内(previous 未过期)解密旧密钥,投递时双签。旧密钥只是接收端
                // 迁移期的附加便利:解密失败(SECRET_KEY 轮换 / 密文损坏)不应拖垮用当前
                // 密钥本可正常签发的投递 —— 退化为单签 + 记 warn,而非整条 dead。
                let previous_secret = match (
                    &endpoint.previous_secret_encrypted,
                    endpoint.previous_secret_expires_at,
                ) {
                    (Some(encrypted), Some(expires_at)) if expires_at > Utc::now() => {
                        match self.secret_key.decrypt(encrypted) {
                            Ok(secret) => Some(secret),
                            Err(err) => {
                                warn!(
                                    error = %err,
                                    "previous webhook secret decrypt failed; delivering single-signed"
                                );
                                None
                            }
                        }
                    }
                    _ => None,
                };
                DeliveryTarget::Webhook {
                    provider_kind: endpoint.provider_kind.map(Into::into).unwrap_or_default(),
                    url: endpoint.url,
                    secret,
                    previous_secret,
                }
            }
        };

        Ok(DeliveryRequest { event, target })
    }

    async fn mark_success<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
        delivery: notification_delivery::Model,
        outcome: DeliveryOutcome,
    ) -> Result<(), DbErr> {
        let next_attempt = delivery.attempt + 1;
        let delivery_id = delivery.id;
        let mut am = delivery.into_active_model();
        am.status = Set(notification_delivery::DeliveryStatus::Sent);
        am.response_code = Set(outcome.response_code);
        am.attempt = Set(next_attempt);
        am.last_error = Set(None);
        am.next_retry_at = Set(None);
        am.request_body = Set(outcome.request_body.clone());
        am.request_timestamp = Set(outcome.request_timestamp);
        am.request_signature = Set(outcome.request_signature.clone());
        am.response_body = Set(outcome.response_body.clone());
        am.update(db).await?;
        // append-only 尝试历史:本次成功尝试。
        self.record_attempt(
            db,
            delivery_id,
            next_attempt,
            notification_delivery::DeliveryStatus::Sent,
            outcome.response_code,
            outcome.request_timestamp,
            outcome.request_signature,
            outcome.response_body,
            None,
        )
        .await?;
        Ok(())
    }

    async fn mark_failure<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
        delivery: notification_delivery::Model,
        failure: DeliveryFailure,
    ) -> Result<bool, DbErr> {
        let next_attempt = delivery.attempt + 1;
        let delivery_id = delivery.id;
        let exhausted =
            failure.kind == DeliveryFailureKind::Permanent || next_attempt >= self.max_attempts;
        let status = if exhausted {
            notification_delivery::DeliveryStatus::Dead
        } else {
            notification_delivery::DeliveryStatus::Failed
        };
        let mut am = delivery.into_active_model();
        am.status = Set(status);
        am.response_code = Set(failure.response_code);
        am.attempt = Set(next_attempt);
        am.last_error = Set(Some(failure.message.clone()));
        am.next_retry_at = Set((!exhausted).then(|| Utc::now() + retry_delay(next_attempt)));
        am.request_body = Set(failure.request_body.clone());
        am.request_timestamp = Set(failure.request_timestamp);
        am.request_signature = Set(failure.request_signature.clone());
        am.response_body = Set(failure.response_body.clone());
        am.update(db).await?;
        // append-only 尝试历史:本次失败尝试(failed 或 dead)。
        self.record_attempt(
            db,
            delivery_id,
            next_attempt,
            status,
            failure.response_code,
            failure.request_timestamp,
            failure.request_signature,
            failure.response_body,
            Some(failure.message),
        )
        .await?;
        Ok(exhausted)
    }

    /// 往 `notification_delivery_attempt` 插一条 append-only 的尝试记录(同事务)。
    #[allow(clippy::too_many_arguments)]
    async fn record_attempt<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
        delivery_id: Uuid,
        attempt_no: i32,
        status: notification_delivery::DeliveryStatus,
        response_code: Option<i32>,
        request_timestamp: Option<i64>,
        request_signature: Option<String>,
        response_body: Option<String>,
        last_error: Option<String>,
    ) -> Result<(), DbErr> {
        notification_delivery_attempt::ActiveModel {
            id: Set(Uuid::now_v7()),
            delivery_id: Set(delivery_id),
            attempt_no: Set(attempt_no),
            status: Set(status),
            response_code: Set(response_code),
            request_timestamp: Set(request_timestamp),
            request_signature: Set(request_signature),
            response_body: Set(response_body),
            last_error: Set(last_error),
            created_at: Set(Utc::now()),
        }
        .insert(db)
        .await?;
        Ok(())
    }

    /// webhook 投递落终态后回写 endpoint 健康:`healthy`(sent)清 `failing_since`;否则
    /// (dead)记起始时刻,失败超阈值则自动停用(保留 `failing_since` 作「因失败停用」标记)。
    async fn update_endpoint_health<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
        endpoint_id: Uuid,
        healthy: bool,
    ) -> Result<(), DbErr> {
        let Some(endpoint) = webhook_endpoint::Entity::find_by_id(endpoint_id)
            .one(db)
            .await?
        else {
            return Ok(());
        };
        if healthy {
            if endpoint.failing_since.is_some() {
                let mut am = endpoint.into_active_model();
                am.failing_since = Set(None);
                am.update(db).await?;
            }
            return Ok(());
        }
        let now = Utc::now();
        let failing_since = endpoint.failing_since.unwrap_or(now);
        let should_disable = !endpoint.disabled
            && (now - failing_since) >= ChronoDuration::days(AUTO_DISABLE_AFTER_DAYS);
        if endpoint.failing_since.is_none() || should_disable {
            let endpoint_name = endpoint.name.clone();
            let mut am = endpoint.into_active_model();
            am.failing_since = Set(Some(failing_since));
            if should_disable {
                am.disabled = Set(true);
                warn!(
                    endpoint = %endpoint_id,
                    name = %endpoint_name,
                    "webhook endpoint auto-disabled after sustained delivery failure"
                );
            }
            am.update(db).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerStats {
    pub outbox_dispatched: u64,
    pub deliveries_attempted: u64,
}

fn retry_delay(attempt: i32) -> ChronoDuration {
    let exponent = attempt.saturating_sub(1).min(7) as u32;
    ChronoDuration::seconds(30 * 2_i64.pow(exponent))
}

/// 取一条投递关联的 webhook endpoint id(仅 webhook 通道;email 无 endpoint → None)。
fn webhook_endpoint_of(delivery: &notification_delivery::Model) -> Option<Uuid> {
    matches!(
        delivery.channel_kind,
        notification_subscription::NotificationChannelKind::Webhook
    )
    .then_some(delivery.webhook_endpoint_id)
    .flatten()
}

/// Run one iteration of the notification worker. Tests use this to avoid
/// waiting on an interval.
pub async fn run_once(state: &AppState) -> Result<WorkerStats, DbErr> {
    Worker::new(
        state.db.clone(),
        state.secret_key.clone(),
        state.mailer.clone(),
    )
    .run_once()
    .await
}

/// Spawn the periodic background worker. Failures are logged and retried on
/// the next tick; notifications are not allowed to block server startup.
pub fn spawn_tasks(state: AppState) {
    tokio::spawn(async move {
        let worker = Worker::new(
            state.db.clone(),
            state.secret_key.clone(),
            state.mailer.clone(),
        );
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            match worker.run_once().await {
                Ok(stats) if stats.outbox_dispatched > 0 || stats.deliveries_attempted > 0 => {
                    info!(
                        outbox = stats.outbox_dispatched,
                        deliveries = stats.deliveries_attempted,
                        "notification worker tick complete"
                    );
                }
                Ok(_) => {}
                Err(err) => warn!(error = %err, "notification worker tick failed"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_grows_exponentially() {
        assert_eq!(retry_delay(1), ChronoDuration::seconds(30));
        assert_eq!(retry_delay(2), ChronoDuration::seconds(60));
        assert_eq!(retry_delay(3), ChronoDuration::seconds(120));
    }
}
