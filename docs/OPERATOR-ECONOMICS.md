# OPERATOR-ECONOMICS.md — funding the validator set, honestly

Status: working draft. Scope: G2 testnet → mainnet launch window.
Owner: core contributors. Audience: Ravi, prospective operators, InfraSingularity.

---

## 1. The question, stated plainly

Running a validator costs money — VPS or bare metal, ops time, monitoring,
incident response. Testnet tokens are worthless by design. So: **why would
anyone run a JunoClaw validator before there is revenue?**

This is the chicken-and-egg at the center of G2:

```
no usage → no fees → no operator revenue → no operators → no credible set
→ no usage
```

Every new L1 hits this. The honest answer is not "the token will be worth
something" — it is to separate *what must be free* from *what can be paid*,
and to sequence them.

---

## 2. The core separation: consensus vs. services

| Layer | What it is | Should it be paid? |
|---|---|---|
| **Consensus seat** | Signing votes with a threshold share | **No** — a paid seat signs only while the invoice clears. Seats must be mission- or stake-motivated, not rented. |
| **Services** | Explorer/indexer hosting, RPC/enterprise API, 24/7 ops, SLA | **Yes** — a priced, contractual service is legitimate and monetizable *today*. |

The G1 wedge taught this directly: the seat is the audition. An operator
whose incentive is a retainer leaves when the retainer ends. An operator
whose incentive is *being the reference institutional operator at mainnet*
has a reason to stay that survives the unpaid phase.

**Rule: G2 seats are unpaid for everyone, equally.** The moment one seat is
on retainer while others volunteer, the set is two-tier and the published
decentralization claims are false.

---

## 3. Five operator-funding paths

Ordered by when they can realistically activate.

### Path A — Volunteer seats (active now)

External operators join G2 unpaid, exactly as G1's four operators did.
Motivation is reputational: early operators are listed in the genesis
validator set documentation, get first-mover operational knowledge of a
PQ-final chain, and position themselves for the services contracts that
*do* pay later.

- **Cost to foundation:** $0
- **Honest framing:** "be early, prove the operations, earn the right to
  the commercial role"
- **Reality check:** this is how every successful testnet bootstrapped.
  The pool is smaller for a young chain — quality over quantity.

### Path B — Deferred services contract (available at funding)

Scope the commercial surface *now*, sign terms that activate at foundation
funding or mainnet launch, whichever comes first. The InfraSingularity
proposal is the template: $5k/mo for hosted explorer/indexer, enterprise
RPC, 24/7 ops, SLA — but the contract is dormant until money exists.

- **Cost now:** $0 (contractual commitment only)
- **What it buys:** operator certainty + a commercial roadmap they can
  plan a team around
- **Key adjustment:** the open-source explorer/archiver/status monitor are
  already shipped — the paid surface is indexer/RPC/enterprise-API
  hosting, not re-selling what the community already has.

### Path C — Validator subsistence grants (at foundation funding)

Once a foundation treasury exists (ecosystem grant, strategic round, or
Junoadjacent community funding), small per-seat stipends retro-compensate
G2 operators — e.g., covering VPS costs (~$50-150/mo/seat) rather than
salaries. Keeps the unpaid-seat principle while recognizing real costs.

- **Cost:** ~$200/mo/seat × ~10 seats ≈ $2k/mo — a rounding error vs.
  a services retainer
- **Framing:** cost reimbursement, not payment-for-consensus

### Path D — Gas fees + commission (at mainnet)

The long-run model: validators earn a share of transaction fees, plus
delegation commission if/when delegation lands. Requires real usage —
which is exactly why it can't fund the testnet phase, and why Paths A-C
exist.

- **Gate:** this is honest revenue *because* it's usage-dependent.
  No usage, no fees — the chain earns before the operators do.

### Path E — Airdrop allocation for early operators (at mainnet)

Reserve a slice of the genesis distribution for verified G2 operators
(e.g., per-seat, weighted by uptime attestations). Converts "worked for
free" into "earned a stake" — arguably the *most* aligned compensation,
because it pays in the asset they helped bootstrap.

- **Cost:** supply allocation, not cash
- **Synergy:** operators who hold the airdrop are economically bound to
  the chain's post-launch success — better than a retainer

---

## 4. Bootstrap timeline

```
NOW (G2 testnet)
  └─ Path A: unpaid seats, published terms, everyone equal
  └─ Path B: scope + sign dormant services contracts
  └─ Publish this document — operators see the full roadmap

FUNDING EVENT (grant, strategic round, or community treasury)
  └─ Path C activates: subsistence grants for verified G2 operators
  └─ Path B contracts activate: paid services begin

MAINNET
  └─ Path D: gas fees + commission become the revenue base
  └─ Path E: operator airdrop allocation vests for G2 participants
  └─ Path C sunsets — subsistence grants end once fees exist
```

The sequence is the point: **never pay for consensus directly; always
pay for the layers around it, or in the asset itself.**

---

## 5. Ravi's questions, answered

**"Why would anyone run a validator for free?"**
Because the seat isn't the product. Being a G2 operator is the credential
that wins the paid services contracts (Path B) and the genesis allocation
(Path E). It's also ~$100/mo of VPS — cheap entry to a permanent role in
a PQ-final chain's validator set.

**"What stops InfraSingularity (or anyone) from walking away when unpaid?"**
Nothing — and that's the correct answer. An operator who needs a retainer
to run a testnet seat is telling you they'll leave the moment the retainer
does. The unpaid phase *filters for* operators with real motivation.

**"Is $5k/mo for services reasonable?"**
At mainnet with real traffic, a 24/7 ops + SLA + enterprise-RPC contract
at that price is market-rate or cheap. At testnet with no traffic, it
buys nothing the open-source tools don't already do. Same price, wrong
timing — hence Path B (sign now, activate at funding).

**"When do operators actually get paid?"**
Path C at first funding (subsistence), Path D at mainnet (fees),
Path E at genesis (airdrop). Between now and funding: they don't, and the
documentation says so plainly. Honesty here is the filter — G2 wants
operators who read this and join anyway.

**"What if we can't get enough unpaid seats?"**
Then the chicken-and-egg is telling us something true: demand doesn't yet
justify the set size. Run a smaller credible set (3-5 external operators
with published failure-domain diversity) rather than a larger padded one.
A 5-seat set that's honest beats a 15-seat set that's rented.

---

## 6. What we say publicly

- G2 seats are unpaid — stated in `G2_VALIDATOR_ONBOARDING.md`
- The services layer monetizes separately — stated in this doc and the
  InfraSingularity counter-proposal
- The operator airdrop allocation is **planned, not committed** — labeled
  PLANNED until governance ratifies it
- Subsistence grants are contingent on funding — labeled PLANNED
- The fee/commission model activates at mainnet — labeled PLANNED

Nothing in this document promises revenue. It promises a path.

---

*Draft — for Ravi's review when he's back from travel.*
