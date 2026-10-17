# Funding Structure — operators, grants, treasury

*Working draft. Connects to the DAO plan in `DAO-ON-JUNOCLAW.md` —
the Treasury SubDAO is the eventual on-chain home for everything here.*

## Why now

G2 proved the model: five independent operators run the set on their
own infrastructure, coordinated through a ceremony, no shared keys.
That only scales if operator costs are covered predictably — otherwise
the set quietly re-centralizes onto whoever can eat VPS bills
indefinitely. Credibility requires the funding story to be as real as
the consensus story.

## Three buckets

### 1. Operator infrastructure costs (monthly, predictable)

| Seat tier | What's covered | Est. cost/mo |
|-----------|----------------|--------------|
| Validator (baseline) | VPS/dedicated running `slay3rd` + monitoring | ~$30–80 (Hetzner/netcup/OVH class) |
| Validator + donor | also serves state-sync snapshots + gRPC | + egress/storage, ~$60–120 |
| Public infra | RPC, indexer/archiver, explorer, faucet, status | ~$100–200 |

Mechanics: flat monthly reimbursement per seat, denominated in fiat
terms, paid by the treasury. No per-block accounting. Eligibility =
seat actually signing (verifiable from cert bitmaps — already
auditable on-chain) + passing the operator checklist
(`g2-window/OPERATOR-CHECKLIST.md`).

### 2. Grants (event-driven, scoped)

Known asks already in flight:

- **MAYO audit** — the big one; see `MAYO-AUDIT-PLAN.md` §cost.
- **Explorer/indexer polish**, faucet, monitoring tooling.
- **Contract ports** (DAO DAO gap fixes, if any land on our side).
- **Ceremony ops** — coordinator time for each reshare window.

Mechanics: scoped proposal → milestone payment. Audit funding should
be treated as a capital item, not monthly opex.

### 3. Treasury (the pool itself)

Sources, in rough order of honesty/simplicity:

1. **Sponsor funding** (today — founder/company covers costs).
2. **Genesis allocation** (mainnet) — a treasury tranche with a
   published vesting/spend policy, governed by Treasury SubDAO.
3. **Later:** ecosystem grants, IBC fee/treasury flow once the
   protocol has fee surfaces. Do not promise these before they exist.

## Governance path

```
today:    sponsor pays; public spreadsheet of what's covered →
P1:       Treasury SubDAO on DAO DAO (operator multisig as interim
          members) ratifies the reimbursement schedule →
mainnet:  genesis tranche funds the SubDAO; spend via proposals
```

The interim structure deliberately mirrors the end state: operators
already act as the multisig-ish group; moving to a real Treasury
SubDAO changes the enforcement, not the social contract.

## Open questions

- Fiat terms vs. token-denominated reimbursement (token needs a
  valuation story — likely fiat-equivalent until mainnet).
- Reimbursement cadence (monthly in arrears, on attestation).
- Who signs off spend before the Treasury SubDAO exists — interim:
  documented sponsor decision + public record.
- Whether public-infra seats (RPC/explorer) are operator duties or a
  separately funded role.
