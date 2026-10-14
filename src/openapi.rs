// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Bill8:Risk
// SPDX-License-Identifier: Apache-2.0

//! OpenAPI spec for the Bill8:Risk API.

use utoipa::OpenApi;

use crate::handlers;

/// The risk service's OpenAPI document.
#[derive(OpenApi)]
#[openapi(paths(
    handlers::create_rule,
    handlers::list_rules,
    handlers::screen,
    handlers::list_reviews,
    handlers::resolve_review,
))]
pub struct RiskOpenApi;
