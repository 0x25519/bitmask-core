#![allow(unused_imports)]
#![cfg(feature = "server")]
#![cfg(not(target_arch = "wasm32"))]
use std::{env, net::SocketAddr, str::FromStr};

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::Path,
    headers::{authorization::Bearer, Authorization},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router, TypedHeader,
};
use bitcoin_30::secp256k1::{ecdh::SharedSecret, PublicKey, SecretKey};
use bitmask_core::{
    rgb::{
        accept_transfer, create_invoice, create_psbt, issue_contract, list_contracts,
        list_interfaces, list_schemas, pay_asset,
    },
    structs::{AcceptRequest, InvoiceRequest, IssueRequest, PsbtRequest, RgbTransferRequest},
};
use log::info;
use tokio::fs;
use tower_http::cors::CorsLayer;

async fn issue(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(issue): Json<IssueRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /issue {issue:?}");

    let nostr_hex_sk = auth.token();

    let issue_res = issue_contract(
        nostr_hex_sk,
        &issue.ticker,
        &issue.name,
        &issue.description,
        issue.precision,
        issue.supply,
        &issue.seal,
        &issue.iface,
    )
    .await?;

    Ok((StatusCode::OK, Json(issue_res)))
}

async fn invoice(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(invoice): Json<InvoiceRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /invoice {invoice:?}");

    let nostr_hex_sk = auth.token();

    let invoice_res = create_invoice(
        nostr_hex_sk,
        &invoice.contract_id,
        &invoice.iface,
        invoice.amount,
        &invoice.seal,
    )
    .await?;

    Ok((StatusCode::OK, Json(invoice_res)))
}

async fn psbt(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(psbt_req): Json<PsbtRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /psbt {psbt_req:?}");

    let nostr_hex_sk = auth.token();

    let psbt_res = create_psbt(nostr_hex_sk, psbt_req).await?;

    Ok((StatusCode::OK, Json(psbt_res)))
}

#[axum_macros::debug_handler]
async fn pay(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(pay_req): Json<RgbTransferRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /pay {pay_req:?}");

    let nostr_hex_sk = auth.token();

    let transfer_res = pay_asset(nostr_hex_sk, pay_req).await?;

    Ok((StatusCode::OK, Json(transfer_res)))
}

async fn accept(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Json(accept_req): Json<AcceptRequest>,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /accept {accept_req:?}");

    let nostr_hex_sk = auth.token();

    let transfer_res = accept_transfer(nostr_hex_sk, accept_req).await?;

    Ok((StatusCode::OK, Json(transfer_res)))
}

async fn contracts(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /contracts");

    let nostr_hex_sk = auth.token();

    let contracts_res = list_contracts(nostr_hex_sk).await?;

    Ok((StatusCode::OK, Json(contracts_res)))
}

async fn interfaces(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /interfaces");

    let nostr_hex_sk = auth.token();

    let interfaces_res = list_interfaces(nostr_hex_sk).await?;

    Ok((StatusCode::OK, Json(interfaces_res)))
}

async fn schemas(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /schemas");

    let nostr_hex_sk = auth.token();

    let schemas_res = list_schemas(nostr_hex_sk).await?;

    Ok((StatusCode::OK, Json(schemas_res)))
}

async fn co_store(
    Path((pk, name)): Path<(String, String)>,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    info!("POST /carbonado/{pk}/{name}, {} bytes", body.len());

    let path = format!("/tmp/bitmaskd/carbonado/{pk}");
    let filename = format!("{path}/{name}");

    fs::create_dir_all(path).await?;
    info!("write {} bytes to {}", body.len(), filename);
    fs::write(filename, body).await?;

    Ok(StatusCode::OK)
}

async fn co_retrieve(
    Path((pk, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    info!("GET /carbonado/{pk}/{name}");

    let path = option_env!("CARBONADO_DIR").unwrap_or("/tmp/bitmaskd/carbonado");
    let filename = format!("{path}/{pk}/{name}");

    info!("read {}", filename);
    let bytes = fs::read(filename).await?;

    Ok((StatusCode::OK, bytes))
}

async fn key(Path(pk): Path<String>) -> Result<impl IntoResponse, AppError> {
    let sk = env::var("NOSTR_SK")?;
    let sk = SecretKey::from_str(&sk)?;

    let pk = PublicKey::from_str(&pk)?;

    let ss = SharedSecret::new(&pk, &sk);
    let ss = ss.display_secret();

    Ok(ss.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }

    pretty_env_logger::init();

    let app = Router::new()
        .route("/issue", post(issue))
        .route("/invoice", post(invoice))
        .route("/psbt", post(psbt))
        .route("/pay", post(pay))
        .route("/accept", post(accept))
        .route("/contracts", get(contracts))
        .route("/interfaces", get(interfaces))
        .route("/schemas", get(schemas))
        .route("/key/:pk", get(key))
        .route("/carbonado/:pk/:name", post(co_store))
        .route("/carbonado/:pk/:name", get(co_retrieve))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([127, 0, 0, 1], 7070));

    info!("bitmaskd REST server successfully running at {addr}");

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

// https://github.com/tokio-rs/axum/blob/fef95bf37a138cdf94985e17f27fd36481525171/examples/anyhow-error-response/src/main.rs
// Make our own error that wraps `anyhow::Error`.
struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
