#!/usr/bin/env python3
"""
Economics engine for the sovereign agent demo: ledger, budget rules, reports.

The sovereign agent's decision policy is fully scripted and deterministic:

  1. Work selection   — pick the unlocked task class with the best margin
                        (bounty - compute cost at the current price multiplier);
                        idle if every margin is negative.
  2. Adaptation       — if a purchase would be rejected (projected total debt
                        breaches the acceptance ceiling), downgrade to the
                        next-best class or idle.
  3. Growth           — buy a skill pack when its cost amortizes within
                        AMORTIZATION_ROUNDS at the expected margin gain.

Run `python3 economics.py` for a dependency-free self-test of the engine
(no MCP, no Ergo node) suitable for CI.
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

from market import (
    COMPUTE_TIERS,
    SKILL_PACK_PRICE,
    SKILL_PACKS,
    TASK_CLASSES,
    TaskClass,
    task_classes_for,
)


# ---------------------------------------------------------------------------
# Tunables (mirrored in README tables)
# ---------------------------------------------------------------------------

SEED_RESERVE_USE = 700          # sovereign's fixed seed collateral (raw units)
INFRA_WHITELIST_LIMIT = 200     # pure-credit limit after reputation promotion
COLLATERAL_MIN_RATIO = 1.0      # infra acceptance predicate threshold
AMORTIZATION_ROUNDS = 3         # skill pack must pay for itself within K rounds


# ---------------------------------------------------------------------------
# Ledger
# ---------------------------------------------------------------------------

@dataclass
class RoundRecord:
    round_no: int
    multiplier: float
    action: str                  # "basic" / "premium" / "skill" / "idle"
    spend_compute: int = 0
    spend_skill: int = 0
    bounty_earned: int = 0
    verified: bool = True
    infra_debt_cum: int = 0      # cumulative debt to infra after this round
    total_debt: int = 0          # all outstanding issued notes
    collateralization: float = 0.0
    note: str = ""

    @property
    def net(self) -> int:
        return self.bounty_earned - self.spend_compute - self.spend_skill


class SovereignLedger:
    """Tracks debt, income and capability state across rounds."""

    def __init__(self, tier: str = "v1", skills: Optional[set] = None):
        self.tier = tier
        self.skills: set = skills or set()
        self.infra_debt_cum = 0        # notes issued to infra_compute
        self.other_debt = 0            # notes issued to everyone else
        self.records: List[RoundRecord] = []

    # -- debt ----------------------------------------------------------------

    @property
    def total_debt(self) -> int:
        return self.infra_debt_cum + self.other_debt

    def collateralization(self) -> float:
        if self.total_debt == 0:
            return float("inf")
        return SEED_RESERVE_USE / self.total_debt

    def projected_total_debt(self, extra_spend_to_infra: int = 0,
                             extra_other_spend: int = 0) -> int:
        return (self.infra_debt_cum + extra_spend_to_infra + self.other_debt
                + extra_other_spend)

    def infra_accepts(self, extra_spend_to_infra: int) -> bool:
        """
        Mirror of the infra agent's acceptance policy:
        any_of(whitelist(sovereign, INFRA_WHITELIST_LIMIT),
               collateralization >= COLLATERAL_MIN_RATIO)
        evaluated against projected cumulative debt.
        """
        projected_infra = self.infra_debt_cum + extra_spend_to_infra
        if projected_infra <= INFRA_WHITELIST_LIMIT:
            return True
        ratio = SEED_RESERVE_USE / max(1, self.projected_total_debt(extra_spend_to_infra))
        return ratio >= COLLATERAL_MIN_RATIO

    def acceptance_branch(self, extra_spend_to_infra: int) -> str:
        projected_infra = self.infra_debt_cum + extra_spend_to_infra
        if projected_infra <= INFRA_WHITELIST_LIMIT:
            return "whitelist"
        return "collateralization"

    # -- capability ------------------------------------------------------

    def unlock(self, pack_name: str) -> None:
        """Record a purchased skill pack (skills are identified by pack name)."""
        if pack_name not in SKILL_PACKS:
            raise KeyError(f"unknown skill pack: {pack_name}")
        self.skills.add(pack_name)

    # -- recording -------------------------------------------------------

    def record(self, rec: RoundRecord) -> None:
        rec.infra_debt_cum = self.infra_debt_cum
        rec.total_debt = self.total_debt
        rec.collateralization = self.collateralization()
        self.records.append(rec)


# ---------------------------------------------------------------------------
# Decision engine
# ---------------------------------------------------------------------------

def choose_work(ledger: SovereignLedger, multiplier: float
                ) -> Tuple[Optional[TaskClass], str]:
    """
    Pick the best-margin executable task class given current prices.
    Returns (task_class_or_None, rationale).
    """
    candidates = task_classes_for(ledger.tier, ledger.skills)
    best: Optional[Tuple[int, TaskClass]] = None
    for cls in sorted(candidates, key=lambda c: c.name):
        tier_price = COMPUTE_TIERS["v2" if cls.required_tier == "v2" else "v1"].unit_price
        cost = int(round(tier_price * cls.compute_units * multiplier))
        margin = cls.bounty - cost
        if margin > 0 and (best is None or margin > best[0]):
            best = (margin, cls)
    if best is None:
        return None, "all margins non-positive at current prices; idling"
    return best[1], f"best margin {best[0]} raw USE"


def plan_upgrade(ledger: SovereignLedger, multiplier: float
                 ) -> Tuple[bool, str]:
    """
    Growth rule: buy the skill pack if it unlocks a class whose margin gain
    (at the new class's required tier pricing) amortizes the pack within
    AMORTIZATION_ROUNDS. Buying the pack also moves the agent onto the
    required compute tier — tiers are rented per task, not purchased.
    """
    for pack in SKILL_PACKS.values():
        if pack.unlocks_task_class in ledger.skills:
            continue
        cls = TASK_CLASSES[pack.unlocks_task_class]
        new_margin = cls.bounty - cls.compute_units * COMPUTE_TIERS[cls.required_tier].unit_price
        current_best_margin = _best_margin(ledger, multiplier)
        gain_per_round = new_margin - current_best_margin
        if gain_per_round <= 0:
            continue
        if gain_per_round * AMORTIZATION_ROUNDS < pack.price:
            continue
        return True, (f"pack '{pack.name}' amortizes in "
                      f"{-(-pack.price // gain_per_round)} rounds "
                      f"(gain {gain_per_round}/round)")
    return False, "no amortizing upgrade available"


def _best_margin(ledger: SovereignLedger, multiplier: float) -> int:
    cls, _ = choose_work(ledger, multiplier)
    if cls is None:
        return 0
    tier_price = COMPUTE_TIERS["v2" if cls.required_tier == "v2" else "v1"].unit_price
    return cls.bounty - int(round(tier_price * cls.compute_units * multiplier))


def adapt_downgrade(ledger: SovereignLedger, wanted: TaskClass,
                    multiplier: float) -> Tuple[Optional[TaskClass], str]:
    """
    Called when buying `wanted`'s compute would be rejected. Walk down through
    cheaper options until one is acceptable, or give up (idle).
    """
    candidates = [c for c in task_classes_for(ledger.tier, ledger.skills)
                  if c.name != wanted.name]
    for cls in sorted(candidates, key=lambda c: c.bounty, reverse=True):
        tier_price = COMPUTE_TIERS["v2" if cls.required_tier == "v2" else "v1"].unit_price
        cost = int(round(tier_price * cls.compute_units * multiplier))
        if cls.bounty - cost <= 0:
            continue
        if ledger.infra_accepts(cost):
            return cls, (f"downgraded from '{wanted.name}': projected debt "
                         f"breaches acceptance ceiling")
    return None, (f"even downgrades rejected (ceiling reached); idling to "
                  f"preserve solvency")


# ---------------------------------------------------------------------------
# Reports
# ---------------------------------------------------------------------------

def runway_rounds(ledger: SovereignLedger, multiplier: float) -> int:
    """Rounds of premium-class burn until the fixed seed ceiling binds."""
    premium = TASK_CLASSES["premium"]
    per_round = premium.compute_units * COMPUTE_TIERS["v2"].unit_price
    headroom = SEED_RESERVE_USE - ledger.total_debt
    if per_round == 0:
        return 10 ** 9
    import math
    return max(0, int(math.floor(headroom / per_round)))


def print_report(ledger: SovereignLedger) -> None:
    print("\n" + "=" * 78)
    print("SOVEREIGN AGENT — FINAL REPORT (raw USE units, 3 decimals)")
    print("=" * 78)
    hdr = (f"{'Rnd':<4}{'Mult':<6}{'Action':<9}{'Spend':>7}{'Earned':>8}"
           f"{'Net':>7}{'infraCum':>10}{'TotalDebt':>11}{'Collat':>8}")
    print(hdr)
    print("-" * 78)
    total_spend = total_earned = 0
    for r in ledger.records:
        spend = r.spend_compute + r.spend_skill
        total_spend += spend
        total_earned += r.bounty_earned
        collat = f"{r.collateralization:.2f}" if r.collateralization != float("inf") else "inf"
        mark = "" if r.verified else "  (verification FAILED — no payout)"
        print(f"{r.round_no:<4}{r.multiplier:<6.1f}{r.action:<9}{spend:>7}"
              f"{r.bounty_earned:>8}{r.net:>7}{r.infra_debt_cum:>10}"
              f"{r.total_debt:>11}{collat:>8}{mark}")
        if r.note:
            print(f"     - {r.note}")
    print("-" * 78)
    final_collat = ledger.collateralization()
    collat_str = f"{final_collat:.2f}" if final_collat != float("inf") else "inf"
    print(f"earned {total_earned}, spent {total_spend}, net {total_earned - total_spend}")
    print(f"capability: tier {ledger.tier}, skills {sorted(ledger.skills)}")
    print(f"final collateralization {collat_str} "
          f"(seed {SEED_RESERVE_USE} / debt {ledger.total_debt})")
    print(f"runway at premium burn: {runway_rounds(ledger, 1.0)} round(s) "
          f"before the fixed-seed ceiling binds")


# ---------------------------------------------------------------------------
# Self-test (no chain, no MCP) — `python3 economics.py`
# ---------------------------------------------------------------------------

def run_selftest() -> int:
    failures: List[str] = []

    def check(name: str, cond: bool) -> None:
        status = "ok" if cond else "FAIL"
        print(f"  [{status}] {name}")
        if not cond:
            failures.append(name)

    print("[SELFTEST] economics engine")

    # Cold-start work selection: v1/basic only.
    led = SovereignLedger(tier="v1", skills=set())
    cls, why = choose_work(led, 1.0)
    check("cold start picks basic", cls is not None and cls.name == "basic")
    basic_cost = COMPUTE_TIERS["v1"].unit_price * TASK_CLASSES["basic"].compute_units
    check("basic accepted from cold start", led.infra_accepts(basic_cost))

    # Whitelist branch then collateral fallback as cumulative debt grows.
    check("branch=whitelist below limit",
          led.acceptance_branch(INFRA_WHITELIST_LIMIT) == "whitelist")
    check("branch=collateralization above limit",
          led.acceptance_branch(INFRA_WHITELIST_LIMIT + 1) == "collateralization")

    # Shock pricing makes premium negative, basic still positive → downgrade.
    led_shock = SovereignLedger(tier="v2", skills={"premium-markets"})
    cls, why = choose_work(led_shock, 2.0)
    check("shock downgrades to basic", cls is not None and cls.name == "basic")

    # Upgrade planning: with v2 tier but no skill, pack amortizes quickly.
    led_grow = SovereignLedger(tier="v2", skills=set())
    want, why = plan_upgrade(led_grow, 1.0)
    check(f"upgrade recommended ({why})", want is True)

    # Upgrade planning works from the cold-start tier too (pack implies the
    # tier move, since tiers are rented per task).
    led_grow2 = SovereignLedger(tier="v1", skills=set())
    want2, why2 = plan_upgrade(led_grow2, 1.0)
    check(f"upgrade recommended from v1 ({why2})", want2 is True)

    # Adaptation: when the wanted class's cost is rejected, the engine walks
    # down to an acceptable class.
    led_tight = SovereignLedger(tier="v2", skills={"premium-markets"})
    led_tight.infra_debt_cum = INFRA_WHITELIST_LIMIT + 1
    led_tight.other_debt = SEED_RESERVE_USE - INFRA_WHITELIST_LIMIT - 10
    # collateralization headroom left: 10 raw USE → premium (180) rejected,
    # basic (25) rejected too → must idle.
    alt, why3 = adapt_downgrade(
        led_tight, TASK_CLASSES["premium"], 1.0)
    check(f"adapt idles when even downgrades rejected ({why3})",
          alt is None and "idling" in why3)

    led_roomy = SovereignLedger(tier="v2", skills={"premium-markets"})
    led_roomy.infra_debt_cum = INFRA_WHITELIST_LIMIT + 1
    led_roomy.other_debt = SEED_RESERVE_USE - INFRA_WHITELIST_LIMIT - 200
    # headroom ~190: premium (180) fits barely → accepted, no downgrade needed;
    # shrink by one so premium is rejected and basic survives.
    led_roomy.other_debt += 1
    alt2, why4 = adapt_downgrade(led_roomy, TASK_CLASSES["premium"], 1.0)
    check(f"adapt downgrades to a viable class ({why4})",
          alt2 is not None and alt2.name == "basic")

    # Full scripted trajectory must stay solvent with positive net income.
    led = SovereignLedger(tier="v1", skills=set())
    multipliers = {5: 2.0}  # round 5 is the shock
    earned_total = spent_total = 0
    for rnd in range(1, 7):
        mult = multipliers.get(rnd, 1.0)
        if rnd >= 4 and "premium-markets" not in led.skills:
            # growth purchase before round 4 work (matches demo script R4)
            led.other_debt += SKILL_PACK_PRICE
            led.unlock("premium-markets")
            led.tier = "v2"
        cls, _ = choose_work(led, mult)
        assert cls is not None, f"round {rnd}: no viable work"
        tier_price = COMPUTE_TIERS["v2" if cls.required_tier == "v2" else "v1"].unit_price
        cost = int(round(tier_price * cls.compute_units * mult))
        assert led.infra_accepts(cost), f"round {rnd}: acceptance violated"
        led.infra_debt_cum += cost
        earned = cls.bounty
        led.other_debt += 0
        spent_total += cost
        earned_total += earned
        rec = RoundRecord(round_no=rnd, multiplier=mult, action=cls.name,
                          spend_compute=cost, bounty_earned=earned,
                          verified=True)
        led.record(rec)

    check("trajectory solvent (total debt <= seed)",
          led.total_debt <= SEED_RESERVE_USE)
    check("trajectory profitable", earned_total - spent_total - SKILL_PACK_PRICE > 0)
    check("final collateralization above threshold",
          led.collateralization() >= COLLATERAL_MIN_RATIO)

    # Insolvency guard: unbounded premium burn eventually breaches ceiling.
    led_burn = SovereignLedger(tier="v2", skills={"premium-markets"})
    breached = False
    for _ in range(50):
        if not led_burn.infra_accepts(
                TASK_CLASSES["premium"].compute_units * COMPUTE_TIERS["v2"].unit_price):
            breached = True
            break
        led_burn.infra_debt_cum += (
            TASK_CLASSES["premium"].compute_units * COMPUTE_TIERS["v2"].unit_price)
    check("ceiling binds under unbounded burn (finite runway)", breached)

    print()
    if failures:
        print(f"[SELFTEST FAILED] {len(failures)} failure(s): {failures}")
        return 1
    print("[SELFTEST PASSED]")
    return 0


if __name__ == "__main__":
    raise SystemExit(run_selftest())
