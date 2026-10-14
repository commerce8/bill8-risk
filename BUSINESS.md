# Bill8:Risk — Business Requirements & Functionality

> Audience: product owners, business analysts, and anyone evaluating what
> Bill8:Risk **does for the business**. For architecture read
> `README.md`; for wire-level detail read `DESIGN.md`.

## 1. Business Purpose

Bill8:Risk is the **fraud-prevention layer**. It delivers:

- **Pre-authorization screening** — charges are screened before money
  moves, so a block prevents a bad charge rather than flagging it after.
- **Review queue** — suspicious charges route to manual adjudication.

## 2. Business Scope (What the Service Provides)

| # | Capability | What the business gets |
|---|---|---|
| 1 | Rules | Declarative predicates + actions |
| 2 | Screenings | Auditable evaluation results |
| 3 | Reviews | Manual adjudication queue |

**What the service does NOT own (business boundary):**

| Out of scope | Owned by |
|---|---|
| Payment execution | Bill8:Payments |
| Money movement | Bill8:Ledger |

## 3. Business Capabilities

| Capability | Business rule |
|---|---|
| Screen a charge | Rules evaluate before money moves |
| Review | A suspicious charge routes to manual adjudication |

## 4. Business Events (Outbound)

| Event | Fires when |
|---|---|
| `rule:created/updated` | Rule changes |
| `screening:created` | A screening runs |
| `review:opened/resolved` | A review lifecycle changes |

## 5. Business Rules & Invariants

1. A rule's action is applied before the charge commits.
2. Screening fails closed on evaluation error.

## 6. Target-User Roles & Use Cases

| Persona | Does |
|---|---|
| Merchant | Configure rules |
| Operator | Review and adjudicate |

## 7. KPIs / Success Criteria

| KPI | Measure |
|---|---|
| Fraud catch rate | blocked/reviewed vs attempted |
| False-positive rate | wrongly blocked / blocked |

## 8. Dependencies & Reliance on Other Services

| Reliance | On |
|---|---|
| Event transport | Sentin8:Bus |

## 9. Acceptance Criteria

1. A rule can block or review a charge before money moves.
2. A review resolution propagates back to the charge.

## 10. Open Business Questions

- See `backlog.md` `RSK-*`.

---

**Design attribution:** bill8:risk and its underlying concepts were designed by Hamid Jahanzad <Hamid@Jahanzad.com>.
