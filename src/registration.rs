// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Bill8:Risk
// SPDX-License-Identifier: Apache-2.0

//! Service self-registration data for Bill8:Risk.

use sentin8::registration::{
    AdminScope, AdminUi, AdminUiPage, ErrorCode, M2mGrant, PgRole, PolicySetDecl, Registration,
    SqlMigration,
};
use utoipa::OpenApi;

const PG_ROLES: &[PgRole] = &[PgRole {
    scope: "bill8:risk:read,bill8:risk:write",
    role: "bill8_risk_service",
    bypass_rls: false,
    database: "bill8",
    cross_schema_grants: &[],
}];

const M2M_GRANTS: &[M2mGrant] = &[
    M2mGrant {
        workload: "bill8:risk",
        scopes: &[
            "sentin8:bus:produce",
            "sentin8:bus:consume",
            "bill8:risk:admin",
            "sentin8:stat:measure",
        ],
    },
    M2mGrant {
        workload: "risk",
        scopes: &[
            "sentin8:bus:produce",
            "sentin8:bus:consume",
            "bill8:risk:admin",
            "sentin8:stat:measure",
        ],
    },
    M2mGrant {
        workload: "risk-audit",
        scopes: &["sentin8:sys:audit:write"],
    },
    M2mGrant {
        workload: "bill8:risk-policy",
        scopes: &["sentin8:bus:consume", "sentin8:stat:measure"],
    },
    M2mGrant {
        workload: "risk-policy",
        scopes: &["sentin8:stat:measure"],
    },
    M2mGrant {
        workload: "bill8-risk-tls",
        scopes: &["sentin8:sys:instances:read"],
    },
];

const ADMIN_SCOPES: &[AdminScope] = &[
    AdminScope::Typed {
        role: "bill8:risk:admin",
        scope: "bill8:risk:rules:read",
    },
    AdminScope::Typed {
        role: "bill8:risk:admin",
        scope: "bill8:risk:rules:write",
    },
    AdminScope::Typed {
        role: "bill8:risk:admin",
        scope: "bill8:risk:screenings:read",
    },
    AdminScope::Typed {
        role: "bill8:risk:admin",
        scope: "bill8:risk:reviews:read",
    },
    AdminScope::Typed {
        role: "bill8:risk:admin",
        scope: "bill8:risk:reviews:write",
    },
    AdminScope::Typed {
        role: "bill8:risk:admin",
        scope: "bill8:risk:policy:read",
    },
    AdminScope::Typed {
        role: "bill8:risk:admin",
        scope: "bill8:risk:policy:write",
    },
];

const ERROR_CODES: &[ErrorCode] = &[
    ErrorCode {
        enum_name: "RiskReviewNotFound",
        error_code: "risk_review_not_found",
        status_code: 404,
    },
    ErrorCode {
        enum_name: "RiskReviewAlreadyResolved",
        error_code: "risk_review_already_resolved",
        status_code: 409,
    },
];

const ADMIN_UI_PAGES: &[AdminUiPage] = &[
    AdminUiPage {
        route: "overview",
        label: "Overview",
        ui_endpoint: "/v1/admin/ui/overview",
        data_endpoint: "/v1/admin/overview",
        icon: "0xe1b1",
    },
    AdminUiPage {
        route: "rules",
        label: "Rules",
        ui_endpoint: "/v1/admin/ui/rules",
        data_endpoint: "/v1/admin/rules",
        icon: "0xe8b8",
    },
    AdminUiPage {
        route: "reviews",
        label: "Review Queue",
        ui_endpoint: "/v1/admin/ui/reviews",
        data_endpoint: "/v1/admin/reviews",
        icon: "0xe8b6",
    },
    AdminUiPage {
        route: "screenings",
        label: "Screenings",
        ui_endpoint: "/v1/admin/ui/screenings",
        data_endpoint: "/v1/admin/screenings",
        icon: "0xe5ca",
    },
];

/// The scope-mirror Shield text.
pub const SCOPE_MIRROR_POLICIES: &str = "permit(principal, action == Action::\"bill8:risk:rules:read\", resource) when { principal.scopes.contains(\"bill8:risk:rules:read\") };\npermit(principal, action == Action::\"bill8:risk:rules:read\", resource) when { principal.scopes.contains(\"sentin8:sys:admin\") };\npermit(principal, action == Action::\"bill8:risk:rules:write\", resource) when { principal.scopes.contains(\"bill8:risk:rules:write\") };\npermit(principal, action == Action::\"bill8:risk:rules:write\", resource) when { principal.scopes.contains(\"sentin8:sys:admin\") };\npermit(principal, action == Action::\"bill8:risk:screenings:write\", resource) when { principal.scopes.contains(\"bill8:risk:screenings:write\") };\npermit(principal, action == Action::\"bill8:risk:reviews:read\", resource) when { principal.scopes.contains(\"bill8:risk:reviews:read\") };\npermit(principal, action == Action::\"bill8:risk:reviews:write\", resource) when { principal.scopes.contains(\"bill8:risk:reviews:write\") };\npermit(principal, action == Action::\"bill8:risk:reviews:write\", resource) when { principal.scopes.contains(\"sentin8:sys:admin\") };\n";

const POLICIES: &[PolicySetDecl] = &[PolicySetDecl {
    policy_set_name: "scope-mirror",
    description: "Action-granular scope permits for bill8:risk over the LIVE effective scope set. Step 5 is the only gate.",
    policies: SCOPE_MIRROR_POLICIES,
    schema: None,
    mode: "enforced",
    default_enabled: true,
}];

const SQL_1_INIT: &str = include_str!("../sql/init.sql");

fn openapi_spec() -> serde_json::Value {
    serde_json::to_value(crate::openapi::RiskOpenApi::openapi()).unwrap_or(serde_json::Value::Null)
}

/// Print the registration JSON to stdout.
pub fn print(section: Option<String>) -> anyhow::Result<()> {
    let reg = Registration {
        service_type: "bill8:risk",
        subdomain: "bill8-risk",
        health_path: "/v1/health",
        admin_scopes: ADMIN_SCOPES,
        error_codes: ERROR_CODES,
        consent_providers: &[],
        consent_flows: &[],
        policies: POLICIES,
        relations: &[],
        event_triggers: &[],
        schedules: &[],
        admin_ui: Some(AdminUi {
            label: "Risk",
            icon: "0xe8b8",
            service_flag: "risk",
            pages: ADMIN_UI_PAGES,
        }),
        sql_migrations: vec![SqlMigration {
            version: 1,
            name: "init",
            sql: SQL_1_INIT,
        }],
        pg_roles: PG_ROLES,
        audit_findings: None,
        service_actions: None,
        m2m_grants: Some(M2M_GRANTS),
        openapi_url: "/openapi.json",
        openapi: openapi_spec,
    };
    sentin8::registration::print_registration(section, &reg, openapi_spec)
}
