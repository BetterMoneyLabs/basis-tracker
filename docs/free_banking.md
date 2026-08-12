# Basis and Free Banking

Free banking denotes a monetary system in which private banks issue their own
redeemable notes under ordinary commercial law, with no monopoly note issuer, no
central bank, and no lender of last resort; market forces — above all the
obligation to redeem notes on demand — control the money stock [1]. The
historical record is dominated by Scotland 1716–1845, whose competitive,
branching, unlimited-liability system was remarkably stable [2], and by the
contrasting US "free banking" era (1837–1863), whose "wildcat" failures are now
attributed by most scholars not to freedom of issue but to legal restrictions —
unit banking and mandatory bond collateral valued at par [3][4] (for the
opposing "inherent instability" reading, see [5]). The theory holds that
convertibility plus interbank note exchange discipline issuers: a bank that
over-issues sees its notes returned by rivals through the clearing system,
producing "adverse clearings" that drain its reserves [6], while brand-name
capital makes over-issue self-destructive [7].

Basis is, in effect, a cryptographic free-banking arrangement. Issuers are
private competitive note-issuing banks: each locks collateral in an on-chain
reserve box (the specie reserve) and issues signed IOU notes that circulate
off-chain and are redeemable against the reserve. Where the literature relies on
institutional mechanisms, Basis mechanizes them. The tracker server plays the
role of the Scottish note-exchange system and the clearinghouse: it maintains
the ledger of who owes what to whom, commits it on-chain via AVL+ tree digests,
and enforces redemption discipline automatically — a redemption is only signed
when the reserve can honor it, exactly the "promises to pay that must be met on
demand" obligation Vera Smith identified as the system's core discipline [8].
Acceptance policies are the Klein-style reputation gate made explicit [7]:
instead of relying on brand alone, each note holder declares machine-checkable
terms (collateralization floors, whitelists, debt ceilings) under which they
will treat an issuer's notes as "good money" — an approximation of Gorton's
"no questions asked" par acceptance [9], enforced per-transaction rather than by
assumption.

The redemption-time policy check added to the tracker maps directly onto the
literature's treatment of distress. A redemption that would push another
holder's collateralization below their accepted floor is rejected — the
mechanized equivalent of adverse clearings stopping an over-extended issuer
before the loss is socialized across note holders [6]. When a reserve is already
undercollateralized and every holder's policy is violated, the tracker's FIFO
fallback (only the oldest outstanding note may redeem) replaces panic with an
orderly queue: it is the sequential-service constraint of Diamond–Dybvig [10]
turned from a run incentive into a fair ordering, and a close relative of the
Scottish "option clause" — a contractual, pre-committed deferral of payment
that free-banking scholars defend as a circuit-breaker against self-fulfilling
runs [11][12]. On-chain, where the tracker cannot intervene, the contract falls
back to the raw historical default: first-come-first-served redemption until the
reserve is drained.

Two caveats keep the analogy honest. Classical free banking was
*fractional*-reserve — banks held precautionary reserves against clearing
variability, not full backing [6] — and on this point Basis is closer to
Scottish practice than to a currency board: nothing in the protocol mandates
backing. Neither the reserve contract nor the tracker enforces a minimum
collateralization ratio; an issuer can circulate notes against a thin reserve,
or against no reserve at all as pure credit. Collateralization requirements are
individual, not systemic — each holder's acceptance policy declares the floor
it demands of an issuer, so an issuer's effective backing is whatever the
market of note holders insists on, note by note. Aggregate collateralization is
therefore an emergent outcome: a Basis economy could be fully backed, or could
run mostly on undercollateralized credit, with redemption discipline and
policy-gated acceptance doing the work that reserve requirements did
historically. Second, the historical system's discipline rested on legal
enforceability of contracts and unlimited liability (the Ayr Bank failure of
1772 was absorbed by shareholders, not note holders [2]); Basis substitutes
collateral and cryptographic verification for courts and personal liability,
which removes those failure modes but also removes the discretionary,
judgment-based stabilization that clearinghouses historically provided in
crises [13][14]. For the crypto-side bridge of the literature — stablecoins as
modern private banknotes, and rule-bound supply as engineered scarcity — see
[9][15][16], with Hayek's competing-currencies argument [17] as the common
intellectual root.

## References

1. Selgin, G., & White, L. H. "How Would the Invisible Hand Handle Money?"
   *Journal of Economic Literature*, 1994.
2. White, L. H. *Free Banking in Britain: Theory, Experience and Debate,
   1800–1845*. Cambridge University Press, 1984 (2nd ed., IEA, 1995).
3. Rockoff, H. "Lessons from the American Experience with Free Banking." In
   Capie, F., & Wood, G. E. (eds.), *Unregulated Banking*, Macmillan, 1991.
4. Dwyer, G. P. "Wildcat Banking, Banking Panics, and Free Banking in the
   United States." *Federal Reserve Bank of Atlanta Economic Review*, 1996.
5. Rolnick, A. J., & Weber, W. E. "Inherent Instability in Banking: The Free
   Banking Experience." *Cato Journal*, 1986 (and Minneapolis Fed working
   papers, 1982–84).
6. Selgin, G. *The Theory of Free Banking: Money Supply under Competitive Note
   Issue*. Rowman & Littlefield / Cato Institute, 1988.
7. Klein, B. "The Competitive Supply of Money." *Journal of Money, Credit and
   Banking*, 1974.
8. Smith, V. C. *The Rationale of Central Banking and the Free Banking
   Alternative*. P. S. King, 1936 (reprinted Liberty Fund, 1990).
9. Gorton, G. B., & Zhang, J. Y. "Taming Wildcat Stablecoins." *University of
   Chicago Law Review* 90(3), 2023.
10. Diamond, D. W., & Dybvig, P. H. "Bank Runs, Deposit Insurance, and
    Liquidity." *Journal of Political Economy*, 1983.
11. Selgin, G., & White, L. H. "The Option Clause in Scottish Banking."
    *Journal of Money, Credit and Banking*, 1997.
12. Selgin, G. "In Defense of Bank Suspension." *Journal of Financial Services
    Research*, 1993.
13. Timberlake, R. H. "The Central Banking Role of Clearinghouse Associations."
    *Journal of Money, Credit and Banking*, 1984.
14. Gorton, G. "Clearinghouses and the Origin of Central Banking in the United
    States." *Journal of Economic History*, 1985.
15. Selgin, G. "Synthetic Commodity Money." *Journal of Financial Stability*,
    2015.
16. White, L. H. "The Market for Cryptocurrencies." *Cato Journal*, 2015.
17. Hayek, F. A. *The Denationalisation of Money*. Institute of Economic
    Affairs, 1976 (enlarged 3rd ed., 1990).
