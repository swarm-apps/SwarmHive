//! 遥测链路集成测试(`add-telemetry-events`):rollup 双表正确性与幂等、raw 清理、
//! POST /api/v1/events 校验面、查询端点权限门控。
//!
//! Requires Docker. Skipped automatically when unavailable.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
};
use serde_json::{Value, json};
use swarmhive_entity::{
    app, client_event, device_rollup_day, event_rollup_day, release, update_event,
};
use swarmhive_server::config::{
    AppConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig,
};
use swarmhive_server::services::telemetry;
use swarmhive_server::state::AppState;
use swarmhive_server::{build_router, db, services::seed};
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use uuid::Uuid;

struct Boot {
    _container: ContainerAsync<Postgres>,
    router: Router,
    db: DatabaseConnection,
}

async fn boot() -> Option<Boot> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(err) => {
            eprintln!("skipping telemetry_smoke: docker unavailable: {err}");
            return None;
        }
    };
    let host = container.get_host().await.ok()?;
    let port = container.get_host_port_ipv4(5432).await.ok()?;
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");

    let db_cfg = DatabaseConfig {
        url,
        auto_sync: true,
        max_connections: 4,
    };
    let conn = db::connect(&db_cfg).await.expect("connect");
    db::sync_schema(&conn).await.expect("sync");
    seed::run(&conn).await.expect("seed");

    let cfg = AppConfig {
        server: ServerConfig {
            bind: "127.0.0.1:0".into(),
            log_format: LogFormat::Pretty,
            base_url: "http://localhost:5173".into(),
        },
        database: db_cfg,
        telemetry: TelemetryConfig::default(),
        mail: Default::default(),
        secret: Default::default(),
    };
    let state = AppState::new(
        conn.clone(),
        cfg,
        swarmhive_server::crypto::SecretKey::for_tests(),
    );
    Some(Boot {
        _container: container,
        router: build_router(state),
        db: conn,
    })
}

fn post(uri: &str, body: &Value, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::empty()).unwrap()
}

async fn body_json(resp: Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    }
}

fn session_cookie(resp: &Response) -> Option<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with("swarmhive_session="))
        .map(|s| s.split(';').next().unwrap().to_string())
}

async fn setup_and_login(router: &Router) -> String {
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/setup",
            &json!({ "email": "owner@example.com", "display_name": "Owner", "password": "Ownerpassword123!" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/auth/login",
            &json!({ "email": "owner@example.com", "password": "Ownerpassword123!" }),
            None,
        ))
        .await
        .unwrap();
    session_cookie(&resp).expect("login cookie")
}

async fn create_app(router: &Router, cookie: &str, slug: &str) -> Uuid {
    let resp = router
        .clone()
        .oneshot(post(
            "/api/v1/apps",
            &json!({ "slug": slug, "display_name": slug, "platforms": ["tauri-desktop"] }),
            Some(cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "create app");
    let v = body_json(resp).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

/// 直插一条 update_check raw 行(伪造 created_at 以测清理)。
async fn insert_check(
    db: &DatabaseConnection,
    app_id: Uuid,
    version: &str,
    client: &str,
    created_at: chrono::DateTime<Utc>,
) {
    update_event::ActiveModel {
        id: Set(Uuid::now_v7()),
        event_name: Set(update_event::UpdateEventName::UpdateCheck),
        result: Set(update_event::EventResult::UpToDate),
        app_id: Set(app_id),
        release_id: Set(None),
        channel: Set(Some("stable".into())),
        current_version: Set(Some(version.to_string())),
        platform: Set("tauri-desktop".into()),
        target: Set(Some("darwin".into())),
        arch: Set(Some("aarch64".into())),
        abi: Set(None),
        artifact_id: Set(None),
        client_id: Set(Some(client.to_string())),
        created_at: Set(created_at),
    }
    .insert(db)
    .await
    .expect("insert raw check");
}

/// add-dashboard-overview:首页全局 overview 端点。建**两个** app + 各灌 update_check + 一条
/// download_completed → rollup → 断言 app/release 计数与端点一致、`update_checks` **真跨 app
/// 求和**(3+2=5,不是单 app)、`downloads_completed` 非零(验证 bigint cast 的 SUM-with-data)、
/// 趋势非空且与总数自洽。
#[tokio::test]
async fn overview_aggregates_counts_across_apps() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let app1 = create_app(&boot.router, &cookie, "ovapp").await;
    let app2 = create_app(&boot.router, &cookie, "ovapp2").await;

    let now = Utc::now();
    // app1 3 条、app2 2 条 update_check —— 跨 app 共 5,验证 overview 不带 app 过滤。
    for (v, c) in [("1.0.0", "c1"), ("1.0.0", "c2"), ("1.1.0", "c3")] {
        insert_check(&boot.db, app1, v, c, now).await;
    }
    for (v, c) in [("2.0.0", "c4"), ("2.0.0", "c5")] {
        insert_check(&boot.db, app2, v, c, now).await;
    }
    // 一条自报 download_completed(走公开 /events)→ 让 downloads_completed 非零,
    // 覆盖 SUM-with-data 的 bigint cast 解码路径。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/events",
            &json!({
                "event": "download_completed",
                "app": "ovapp",
                "platform": "tauri-desktop",
                "client_id": "dl-device"
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    telemetry::run_rollup(&boot.db).await.expect("rollup");

    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/telemetry/overview?days=30", Some(&cookie)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ov = body_json(resp).await;

    // 计数与直接查库一致(此处恰为 2 app / 0 release)。
    let expected_apps = app::Entity::find().count(&boot.db).await.unwrap() as i64;
    let expected_releases = release::Entity::find().count(&boot.db).await.unwrap() as i64;
    assert_eq!(ov["app_count"].as_i64(), Some(expected_apps));
    assert_eq!(ov["app_count"].as_i64(), Some(2), "both apps counted");
    assert_eq!(ov["release_count"].as_i64(), Some(expected_releases));

    // 期内 update_checks **跨两个 app** 按次求和 = 3+2 = 5;download_completed = 1。
    assert_eq!(ov["update_checks"].as_i64(), Some(5), "summed across apps");
    assert_eq!(ov["downloads_completed"].as_i64(), Some(1));

    // 趋势非空,且趋势两系列求和各自与期内总数自洽。
    let trend = ov["trend"].as_array().expect("trend array");
    assert!(!trend.is_empty(), "trend has at least today's bucket");
    let checks_sum: i64 = trend
        .iter()
        .map(|p| p["update_checks"].as_i64().unwrap_or(0))
        .sum();
    let dl_sum: i64 = trend
        .iter()
        .map(|p| p["downloads_completed"].as_i64().unwrap_or(0))
        .sum();
    assert_eq!(
        checks_sum, 5,
        "trend update_checks sums to the in-period total"
    );
    assert_eq!(dl_sum, 1, "trend downloads sums to the in-period total");
}

#[tokio::test]
async fn rollup_computes_uniques_and_is_idempotent() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let app_id = create_app(&boot.router, &cookie, "rollapp").await;

    // 3 个 distinct client 在 1.0.0(其中 c1 checked 两次),2 个在 1.1.0。
    let now = Utc::now();
    for (v, c) in [
        ("1.0.0", "c1"),
        ("1.0.0", "c1"),
        ("1.0.0", "c2"),
        ("1.0.0", "c3"),
        ("1.1.0", "c4"),
        ("1.1.0", "c5"),
    ] {
        insert_check(&boot.db, app_id, v, c, now).await;
    }

    telemetry::run_rollup(&boot.db).await.expect("rollup");

    let uniques = |version: Option<&str>| {
        let db = boot.db.clone();
        let version = version.map(String::from);
        async move {
            let mut q = device_rollup_day::Entity::find()
                .filter(device_rollup_day::Column::AppId.eq(app_id));
            q = match version {
                Some(v) => q.filter(device_rollup_day::Column::Version.eq(v)),
                None => q.filter(device_rollup_day::Column::Version.is_null()),
            };
            q.one(&db).await.unwrap().map(|r| r.unique_clients)
        }
    };
    assert_eq!(uniques(Some("1.0.0")).await, Some(3));
    assert_eq!(uniques(Some("1.1.0")).await, Some(2));
    assert_eq!(uniques(None).await, Some(5), "version=NULL 总行");

    // 计数表:update_check 共 6 次。
    let total: i64 = event_rollup_day::Entity::find()
        .filter(event_rollup_day::Column::AppId.eq(app_id))
        .filter(event_rollup_day::Column::EventName.eq("update_check"))
        .all(&boot.db)
        .await
        .unwrap()
        .iter()
        .map(|r| r.count)
        .sum();
    assert_eq!(total, 6);

    // 幂等:重跑结果不变(行数与 unique 均不变)。
    telemetry::run_rollup(&boot.db).await.expect("rollup again");
    assert_eq!(uniques(Some("1.0.0")).await, Some(3));
    assert_eq!(uniques(None).await, Some(5));
    let device_rows = device_rollup_day::Entity::find()
        .filter(device_rollup_day::Column::AppId.eq(app_id))
        .count(&boot.db)
        .await
        .unwrap();
    assert_eq!(device_rows, 3, "1.0.0 + 1.1.0 + NULL 总行,无重复");
}

#[tokio::test]
async fn cleanup_deletes_old_raw_but_keeps_rollups() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let app_id = create_app(&boot.router, &cookie, "cleanapp").await;

    // 今天 1 条 + 91 天前 2 条。
    let now = Utc::now();
    insert_check(&boot.db, app_id, "1.0.0", "fresh", now).await;
    insert_check(&boot.db, app_id, "0.9.0", "old1", now - Duration::days(91)).await;
    insert_check(&boot.db, app_id, "0.9.0", "old2", now - Duration::days(91)).await;
    telemetry::run_rollup(&boot.db).await.expect("rollup");
    // 伪造历史 bucket(模拟 91 天前 rollup 早已固化——重算窗口只覆盖近两天)。
    device_rollup_day::ActiveModel {
        id: Set(Uuid::now_v7()),
        app_id: Set(app_id),
        day: Set((now - Duration::days(91)).date_naive()),
        version: Set(Some("0.9.0".into())),
        unique_clients: Set(2),
        created_at: Set(now),
    }
    .insert(&boot.db)
    .await
    .unwrap();

    let deleted = telemetry::run_cleanup(&boot.db, 90).await.expect("cleanup");
    assert_eq!(deleted, 2, "91 天前的 2 条 raw 被删");

    let raw_left = update_event::Entity::find()
        .filter(update_event::Column::AppId.eq(app_id))
        .count(&boot.db)
        .await
        .unwrap();
    assert_eq!(raw_left, 1, "今天的 raw 保留");

    // 历史 rollup 行不受影响(adoption 永久可查)。
    let hist = device_rollup_day::Entity::find()
        .filter(device_rollup_day::Column::AppId.eq(app_id))
        .filter(device_rollup_day::Column::Version.eq("0.9.0"))
        .one(&boot.db)
        .await
        .unwrap();
    assert_eq!(hist.map(|r| r.unique_clients), Some(2));

    // retention=0 → 不清理。
    let deleted = telemetry::run_cleanup(&boot.db, 0).await.expect("noop");
    assert_eq!(deleted, 0);
}

#[tokio::test]
async fn report_event_endpoint_validates_and_stores() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    create_app(&boot.router, &cookie, "evapp").await;

    // 正常上报。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/events",
            &json!({
                "event": "download_completed",
                "app": "evapp",
                "platform": "react-native-android",
                "client_id": "device-1",
                "target_version": "0.3.0",
                "bytes_total": 52428800,
                "duration_ms": 12000
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let row = client_event::Entity::find()
        .filter(client_event::Column::ClientId.eq("device-1"))
        .one(&boot.db)
        .await
        .unwrap()
        .expect("client_event stored");
    assert_eq!(
        row.event_name,
        client_event::ClientEventName::DownloadCompleted
    );
    assert_eq!(row.target_version.as_deref(), Some("0.3.0"));

    // 未知 app → 404。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/events",
            &json!({ "event": "download_started", "app": "ghost", "platform": "x", "client_id": "d" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // 非法 event 名 → 422(serde enum 白名单)。
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/events",
            &json!({ "event": "made_up", "app": "evapp", "platform": "x", "client_id": "d" }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // 超长 error_message 截断到 512。
    let long = "e".repeat(2000);
    let resp = boot
        .router
        .clone()
        .oneshot(post(
            "/api/v1/events",
            &json!({
                "event": "download_failed",
                "app": "evapp",
                "platform": "x",
                "client_id": "device-2",
                "error_message": long
            }),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let row = client_event::Entity::find()
        .filter(client_event::Column::ClientId.eq("device-2"))
        .one(&boot.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.error_message.map(|m| m.len()), Some(512));
}

#[tokio::test]
async fn telemetry_queries_are_gated_and_consistent() {
    let Some(boot) = boot().await else { return };
    let cookie = setup_and_login(&boot.router).await;
    let app_id = create_app(&boot.router, &cookie, "qapp").await;
    let now = Utc::now();
    insert_check(&boot.db, app_id, "1.0.0", "qa-1", now).await;
    insert_check(&boot.db, app_id, "1.0.0", "qa-2", now).await;
    telemetry::run_rollup(&boot.db).await.expect("rollup");

    // owner(telemetry:read)→ 200,序列与 rollup 一致。
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            "/api/v1/telemetry/adoption?app=qapp&days=7",
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let series = body_json(resp).await;
    let arr = series.as_array().unwrap();
    assert!(
        arr.iter()
            .any(|p| p["version"] == "1.0.0" && p["unique_clients"] == 2),
        "adoption contains per-version row: {series:?}"
    );

    // summary 今日活跃 = 2。
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            "/api/v1/telemetry/summary?app=qapp&days=7",
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(body_json(resp).await["active_devices_today"], 2);

    // 匿名 → 401;非法 dim → 422。
    let resp = boot
        .router
        .clone()
        .oneshot(get("/api/v1/telemetry/adoption?app=qapp", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let resp = boot
        .router
        .clone()
        .oneshot(get(
            "/api/v1/telemetry/distribution?app=qapp&dim=bogus",
            Some(&cookie),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
