# Bill8:Risk — Detailed Design Document

## 1. Scope & Context

Bill8:Risk owns: the risk domain schema (domain tables + the leaf
schema `risk` for outbox/audit journal), the rule / review / screening
REST API, the inline screening handler invoked by payments, and the
`bill8:risk:*` event family.

It does NOT own: payment execution (Bill8:Payments), or money
movement (Bill8:Ledger).

```mermaid
graph TD
    PAY["Bill8:Payments"] -->|"screening"| R["Bill8:Risk"]
    BFF["Sentin8:BFF /proxy/risk/*"] --> R
    R -->|"risk.outbox → relay"| BUS["Sentin8:Bus"]
    BUS -->|"review:resolved"| PAY
    R <-->|"schema risk"| PG[("bill8 database")]
```

## 2. Runtime Architecture

Shell-booted; `build_state` adds config, Bus client, AppState.

**Screening (inline, called by payments)**:

```mermaid
sequenceDiagram
    participant P as Bill8:Payments
    participant R as Bill8:Risk
    participant DB as PostgreSQL
    P->>R: POST /v1/screenings (charge context)
    R->>R: evaluate rules (amount/country/velocity)
    R->>DB: screening record + match result
    R-->>P: allow | review | block
    alt review
        R->>DB: review record status=open + outbox review:opened
        R-->>P: review (hold charge)
    end
```

## 3. Data Model

```mermaid
erDiagram
    rules ||--o{ screenings : "evaluated by"
    screenings ||--o{ reviews : "escalate to"
```

| Table | Purpose |
|---|---|
| `rules` | Declarative predicates + actions |
| `screenings` | Evaluation results (auditable) |
| `reviews` | Manual adjudication queue |

**Review state machine**:

```mermaid
stateDiagram-v2
    [*] --> open: escalate
    open --> approved: allow
    open --> declined: block
    approved --> [*]
    declined --> [*]
```

**Producer tables** (platform contracts, `risk` leaf schema):
`risk.outbox` (DBX producer path — architecture-principles §6.7) and
`risk.audit_journal` (the 15-column contract of
`sentin8_audit::audit_journal_sql`, AUDJOUR-1 — env-pinned RLS, drained
to Sys `POST /v1/audit` by the shell's boot-8c pusher).

## 4. API Surface

| Endpoint group | Prefix | Authorization | Events |
|---|---|---|---|
| Rules | `/v1/rules` | `bill8:risk:rules:{read,write}` | `rule:created/updated` |
| Screenings | `/v1/screenings` | `bill8:risk:screenings:read` (write is M2M) | `screening:created` |
| Reviews | `/v1/reviews` | `bill8:risk:reviews:{read,write}` | `review:opened/resolved` |
| Internal | `/v1/internal/*` | M2M | — |

## 5. Cross-Cutting Concerns

- Shield-gated; `env_id` RLS.
- Certificate identity (CERTFREE-4V) (`risk`, `risk-policy`, `risk-audit`).
- Screening fail-closed (deny on evaluation error).
- Review resolution idempotent.

## 6. Failure Modes & Recovery

```mermaid
flowchart TD
    EVAL_FAIL["rule evaluation error"] -->|"fail-closed"| BLOCK["screening returns block"]
    CRASH["crash mid-tx"] -->|"atomic"| RB["screening + review roll back"]
    RELAY_FAIL["Bus outage"] -->|"producer"| OUTBOX["risk.outbox persists — retry ≤10"]
```

## 7. Concurrency & Performance Model

- Pool cap 3; screening is on the payments hot path — evaluation is
  in-memory and pure (no row locks), results persisted best-effort.

## 8. Invariants, Insecure-Defaults Rejected, Unsafe Rationale

- A rule's action is applied before the charge commits.
- Screening fails closed on evaluation error.
- `#![forbid(unsafe_code)]`.

## 9. Testing Strategy

```mermaid
graph TD
    U["Unit — rule predicates, velocity windows"] --> I["Integration — DB-backed screening/review suites"]
    I --> E["E2E — e2e-ts UI + BFF proxy only"]
```

## 10. Open Questions

- See `backlog.md` `RSK-*`.

---

**Design attribution:** bill8:risk and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
