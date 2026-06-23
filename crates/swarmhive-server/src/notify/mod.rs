//! 通知子系统(`add-notifications`)——事件 → 订阅 → 多通道 → 投递。
//!
//! 四层(随 apply increment 逐步落地):
//! - **emit**:业务 handler 在自己事务内写 `notification_outbox`(崩溃不丢、回滚不发)。
//! - **worker**:后台 tokio 任务,`FOR UPDATE SKIP LOCKED` 取 outbox 扇出成 delivery,
//!   再投递 due delivery(指数退避重试 / 死信)。
//! - **channel**:email(复用 `Mailer`)+ outgoing webhook(Standard Webhooks 签名)。
//! - **signer**([`signer`]):Standard Webhooks v1 HMAC-SHA256 签名。

pub mod channel;
pub mod emit;
pub mod providers;
pub mod signer;
pub mod worker;
