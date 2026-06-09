//! 把 admin SPA(`apps/admin/dist`)嵌入 server 二进制并作为 axum fallback 提供。
//!
//! 整个模块仅在 `embed-spa` feature 下编译:`#[derive(RustEmbed)]` 在 `dist`
//! 不存在时会**编译期报错**,而 dev / CI / 集成测试普遍没有构建过 SPA,故默认
//! 关闭;release 容器 / 发布二进制构建前先 `pnpm admin:build` 再 `--features
//! embed-spa`。fallback 只接 axum 未匹配的路由——`/api/*`、`/healthz`、`/api/docs`
//! 这些已注册路由天然优先,不会被它遮蔽。

#![cfg(feature = "embed-spa")]

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

/// 构建期把 `apps/admin/dist`(相对 server crate 的 `CARGO_MANIFEST_DIR`)整目录
/// 嵌入二进制。release 构建直接内联字节,与目标三元组无关 → 跨架构(aarch64)
/// 构建复用同一份 dist。
#[derive(RustEmbed)]
#[folder = "../../apps/admin/dist"]
struct SpaAssets;

/// SPA 根文档:任何非资源路径都回退到它,交给前端 client-side router 渲染。
const INDEX_HTML: &str = "index.html";

/// axum fallback handler:命中嵌入资源按 mime 返回;否则回退 `index.html`(200),
/// 让 `/login`、`/apps/:slug` 这类 SPA 路由的刷新 / 直达也能渲染。
pub async fn fallback_handler(uri: Uri) -> Response {
    // 去掉前导 `/`;根路径直接取 index.html。
    let trimmed = uri.path().trim_start_matches('/');
    let path = if trimmed.is_empty() {
        INDEX_HTML
    } else {
        trimmed
    };

    if let Some(resp) = serve(path) {
        return resp;
    }
    // 资源未命中 → SPA client-side 路由,回退到 index.html。
    serve(INDEX_HTML).unwrap_or_else(|| {
        // 仅当二进制里连 index.html 都没有(理论上不会:dist 一定含 index.html)。
        (
            StatusCode::NOT_FOUND,
            "admin SPA index.html missing from build",
        )
            .into_response()
    })
}

/// 取出嵌入资源并裹成带 `Content-Type` 的 200 响应;资源不存在返回 `None`。
fn serve(path: &str) -> Option<Response> {
    let asset = SpaAssets::get(path)?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .body(Body::from(asset.data.into_owned()))
        .expect("static asset response always builds");
    Some(resp)
}
