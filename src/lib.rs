// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Bill8:Risk
// SPDX-License-Identifier: Apache-2.0

//! # Bill8:Risk - fraud prevention
//!
//! Declarative rules, velocity checks, and a review queue. The rules
//! evaluator is PURE (no I/O, no row locks) so it runs on the payments
//! hot path; evaluation **fails closed** - a malformed rule blocks,
//! never allows. See `DESIGN.md`.

#![forbid(unsafe_code)]

pub mod auth;
pub mod config;
pub mod engine;
pub mod handlers;
pub mod openapi;

// The utoipa dump for registration::print - mirrored from main.rs; the
// lib crate compiles registration::print too (test harnesses embedding
// via #[path] never invoke it, so the pointer is referenced, not run).
static OPENAPI_DUMP: fn() -> serde_json::Value = || {
    use utoipa::OpenApi as _;
    serde_json::to_value(openapi::RiskOpenApi::openapi()).unwrap_or(serde_json::Value::Null)
};

pub mod policy;
pub mod registration;
pub mod server;
