// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Bill8:Risk
// SPDX-License-Identifier: Apache-2.0

//! Risk HTTP handlers — rules CRUD, screening (M2M), review resolution.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use bill8::risk::contracts::{CreateRuleRequest, Review, Rule, ScreenRequest, Screening};
use sqlx::Row;
use uuid::Uuid;

use crate::auth;
use crate::engine::{evaluate, Decision};
use crate::policy::Engine;
use std::sync::Arc;

use crate::config::RiskConfig;

/// Shared application state bound into the shell.
#[derive(Clone)]
pub struct AppState {
    /// Service configuration.
    pub config: Arc<RiskConfig>,
    /// Capped PostgreSQL pool.
    pub pool: sqlx::PgPool,
    /// M2M-backed Bus client.
    pub bus_client: sentin8::bus::client::BusClient,
    /// Shield policy engine — step 5 is the ONLY gate.
    pub policy: Arc<Engine>,
}

/// Set the transaction-local RLS tenant context.
pub async fn set_tenant_context(
    conn: &mut sqlx::PgConnection,
    env_id: uuid::Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT sentin8_set_tenant_context($1, NULL, NULL, NULL, false, false, false, NULL, 'risk')")
        .bind(env_id)
        .execute(conn)
        .await
        .map(|_| ())
}

/// Standard DB error → HTTP 500.
pub fn err_db(e: sqlx::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Health probe payload.
pub async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "risk",
        "version": sentin8::build_info::id(), "build": sentin8::build_info::build_hash() }))
}

/// `POST /v1/rules` — create a rule.
#[utoipa::path(
    post,
    path = "/v1/rules",
    request_body = CreateRuleRequest,
    responses(
        (status = 200, description = "Rule created", body = Rule),
        (status = 403, description = "Missing bill8:risk:rules:write"),
    ),
    security(("bill8:risk:rules:write" = [])),
    tag = "risk",
)]
pub async fn create_rule(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateRuleRequest>,
) -> Result<Json<Rule>, (StatusCode, String)> {
    let a = auth::verify_token(&state.config, &headers).await?;
    crate::policy::require(
        &state.policy,
        &a,
        &headers,
        sentin8::shield::EvalTarget::new("bill8:risk:rules:write"),
    ).await?;

    let mut conn = state.pool.begin().await.map_err(err_db)?;
    set_tenant_context(&mut conn, a.env_id()).await.map_err(err_db)?;

    let id = Uuid::new_v4();
    let row = sqlx::query(
        r#"INSERT INTO risk.rules (id, env_id, predicate, params, action, priority)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, env_id, predicate, params, action, priority, enabled, created_at"#,
    )
    .bind(id)
    .bind(a.env_id())
    .bind(&req.predicate)
    .bind(sqlx::types::Json(&req.params))
    .bind(&req.action)
    .bind(req.priority)
    .fetch_one(&mut *conn)
    .await
    .map_err(err_db)?;

    sqlx::query(
        r#"INSERT INTO risk.outbox (env_id, event_type, payload)
           VALUES ($1, 'bill8:risk:rule:created', $2)"#,
    )
    .bind(a.env_id())
    .bind(serde_json::json!({ "rule_id": id }))
    .execute(&mut *conn)
    .await
    .map_err(err_db)?;

    conn.commit().await.map_err(err_db)?;
    Ok(Json(row_to_rule(&row)))
}

/// `GET /v1/rules` — list rules.
#[utoipa::path(
    get,
    path = "/v1/rules",
    responses(
        (status = 200, description = "List of rules", body = [Rule]),
        (status = 403, description = "Missing bill8:risk:rules:read"),
    ),
    security(("bill8:risk:rules:read" = [])),
    tag = "risk",
)]
pub async fn list_rules(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Rule>>, (StatusCode, String)> {
    let a = auth::verify_token(&state.config, &headers).await?;
    crate::policy::require(
        &state.policy,
        &a,
        &headers,
        sentin8::shield::EvalTarget::new("bill8:risk:rules:read"),
    ).await?;

    let mut conn = state.pool.acquire().await.map_err(err_db)?;
    set_tenant_context(&mut conn, a.env_id()).await.map_err(err_db)?;

    let rows = sqlx::query(
        "SELECT id, env_id, predicate, params, action, priority, enabled, created_at FROM risk.rules ORDER BY priority ASC LIMIT 500",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(err_db)?;

    Ok(Json(rows.iter().map(row_to_rule).collect()))
}

/// `POST /v1/screenings` — screen a charge context (M2M from payments).
/// The evaluator is pure; the outcome persists as an auditable screening.
#[utoipa::path(
    post,
    path = "/v1/screenings",
    request_body = ScreenRequest,
    responses(
        (status = 200, description = "Screening outcome", body = Screening),
        (status = 403, description = "Missing bill8:risk:screenings:write"),
    ),
    security(("bill8:risk:screenings:write" = [])),
    tag = "risk",
)]
pub async fn screen(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ScreenRequest>,
) -> Result<Json<Screening>, (StatusCode, String)> {
    let a = auth::verify_token(&state.config, &headers).await?;
    crate::policy::require(
        &state.policy,
        &a,
        &headers,
        sentin8::shield::EvalTarget::new("bill8:risk:screenings:write"),
    ).await?;

    let mut conn = state.pool.begin().await.map_err(err_db)?;
    set_tenant_context(&mut conn, a.env_id()).await.map_err(err_db)?;

    // Load the ordered rule set and evaluate (pure, fail-closed).
    let rows = sqlx::query(
        "SELECT id, env_id, predicate, params, action, priority, enabled, created_at FROM risk.rules WHERE enabled ORDER BY priority ASC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(err_db)?;
    let rules: Vec<Rule> = rows.iter().map(row_to_rule).collect();

    let decision = evaluate(&rules, &req.charge_context);
    let (outcome, matched_rule_id): (&str, Option<Uuid>) = match decision {
        Decision::Allow => ("allow", None),
        Decision::Review => {
            // Escalate: open a review case for manual adjudication.
            let screening_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO risk.reviews (env_id, screening_id, status) VALUES ($1, $2, 'open')",
            )
            .bind(a.env_id())
            .bind(screening_id)
            .execute(&mut *conn)
            .await
            .map_err(err_db)?;
            ("review", None)
        }
        Decision::Block => ("block", None),
    };

    let id = Uuid::new_v4();
    let row = sqlx::query(
        r#"INSERT INTO risk.screenings (id, env_id, charge_context, outcome, matched_rule_id)
           VALUES ($1, $2, $3, $4, $5)
           RETURNING id, env_id, charge_context, outcome, matched_rule_id, created_at"#,
    )
    .bind(id)
    .bind(a.env_id())
    .bind(sqlx::types::Json(&req.charge_context))
    .bind(outcome)
    .bind(matched_rule_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(err_db)?;

    sqlx::query(
        r#"INSERT INTO risk.outbox (env_id, event_type, payload)
           VALUES ($1, 'bill8:risk:screening:created', $2)"#,
    )
    .bind(a.env_id())
    .bind(serde_json::json!({ "screening_id": id, "outcome": outcome }))
    .execute(&mut *conn)
    .await
    .map_err(err_db)?;

    conn.commit().await.map_err(err_db)?;
    Ok(Json(row_to_screening(&row)))
}

/// `GET /v1/reviews` — list the review queue.
#[utoipa::path(
    get,
    path = "/v1/reviews",
    responses(
        (status = 200, description = "Review queue", body = [Review]),
        (status = 403, description = "Missing bill8:risk:reviews:read"),
    ),
    security(("bill8:risk:reviews:read" = [])),
    tag = "risk",
)]
pub async fn list_reviews(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Review>>, (StatusCode, String)> {
    let a = auth::verify_token(&state.config, &headers).await?;
    crate::policy::require(
        &state.policy,
        &a,
        &headers,
        sentin8::shield::EvalTarget::new("bill8:risk:reviews:read"),
    ).await?;

    let mut conn = state.pool.acquire().await.map_err(err_db)?;
    set_tenant_context(&mut conn, a.env_id()).await.map_err(err_db)?;

    let rows = sqlx::query(
        "SELECT id, env_id, screening_id, status, resolved_at, created_at FROM risk.reviews ORDER BY created_at DESC LIMIT 200",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(err_db)?;

    Ok(Json(
        rows.iter()
            .map(|r| Review {
                id: r.get("id"),
                screening_id: r.get("screening_id"),
                status: r.get("status"),
                resolved_at: r.get("resolved_at"),
                created_at: r.get("created_at"),
            })
            .collect(),
    ))
}

/// `POST /v1/reviews/{id}/resolve` — resolve a review (idempotent).
#[utoipa::path(
    post,
    path = "/v1/reviews/{id}/resolve",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Review resolved"),
        (status = 409, description = "Already resolved"),
        (status = 404, description = "Review not found"),
    ),
    security(("bill8:risk:reviews:write" = [])),
    tag = "risk",
)]
pub async fn resolve_review(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let a = auth::verify_token(&state.config, &headers).await?;
    crate::policy::require(
        &state.policy,
        &a,
        &headers,
        sentin8::shield::EvalTarget::new("bill8:risk:reviews:write"),
    ).await?;

    let decision = body["decision"].as_str().unwrap_or("declined");
    let status = match decision {
        "approve" | "approved" => "approved",
        _ => "declined",
    };

    let mut conn = state.pool.begin().await.map_err(err_db)?;
    set_tenant_context(&mut conn, a.env_id()).await.map_err(err_db)?;

    // Guarded terminal transition — resolving twice is a 409, not a
    // second resolution (exactly-once semantics downstream).
    let result = sqlx::query(
        "UPDATE risk.reviews SET status = $1, resolved_at = now() WHERE id = $2 AND status = 'open'",
    )
    .bind(status)
    .bind(id)
    .execute(&mut *conn)
    .await
    .map_err(err_db)?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::CONFLICT, "review not open".into()));
    }

    // Payload completeness (architecture-principles §4.14): the
    // resolution event carries the CHARGE reference so the payments
    // consumer completes the held charge WITHOUT reading risk tables —
    // the charge id sits in the originating screening's charge context.
    let screening = sqlx::query(
        "SELECT charge_context FROM risk.screenings WHERE id = (SELECT screening_id FROM risk.reviews WHERE id = $1)",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(err_db)?;
    let charge_ref = screening
        .and_then(|row| {
            let ctx = row.get::<sqlx::types::Json<serde_json::Value>, _>("charge_context").0;
            ctx.get("charge_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default();

    sqlx::query(
        r#"INSERT INTO risk.outbox (env_id, event_type, payload)
           VALUES ($1, 'bill8:risk:review:resolved', $2)"#,
    )
    .bind(a.env_id())
    .bind(serde_json::json!({
        "review_id": id,
        "status": status,
        // The held-charge reference (empty when the screening carried
        // none — the consumer ignores resolutions without one).
        "charge_id": charge_ref,
    }))
    .execute(&mut *conn)
    .await
    .map_err(err_db)?;

    conn.commit().await.map_err(err_db)?;
    Ok(Json(serde_json::json!({ "review_id": id, "status": status })))
}

fn row_to_rule(row: &sqlx::postgres::PgRow) -> Rule {
    Rule {
        id: row.get("id"),
        env_id: row.get("env_id"),
        predicate: row.get("predicate"),
        params: row.get::<sqlx::types::Json<serde_json::Value>, _>("params").0,
        action: row.get("action"),
        priority: row.get("priority"),
        enabled: row.get("enabled"),
        created_at: row.get("created_at"),
    }
}

fn row_to_screening(row: &sqlx::postgres::PgRow) -> Screening {
    Screening {
        id: row.get("id"),
        env_id: row.get("env_id"),
        charge_context: row.get::<sqlx::types::Json<serde_json::Value>, _>("charge_context").0,
        outcome: row.get("outcome"),
        matched_rule_id: row.get("matched_rule_id"),
        created_at: row.get("created_at"),
    }
}