# Accounting inventory schema 2

`Node::financial_inventory` attempts a simultaneous snapshot of the BDK wallet,
channel manager, every channel monitor, sweeper, and payment store. The wallet is
acquired first; subsequent component locks are nonblocking. Contention releases
the acquired locks and returns schema-1 non-atomic evidence with an explicit gap.
No network requests, signing, or persistence run inside the snapshot callback.

Schema 2 reports exact pre-fee holder channel allocations in millisatoshis,
separately from outbound encumbrances and conditional inbound HTLCs. Outbound
HTLCs remain inside the holder allocation until irrevocably resolved. Pending
inbound HTLCs are not income. Reserves, possible future closing costs and anchor
fees are not guessed from outbound capacity. Monitor close claims remain separate
from the live channel allocation and must never be added to it.

Payment history is sanitized inside the same boundary: identity, direction,
status, amount, fee, transaction ID/payment hash, and update time. Secrets,
preimages, invoices, and payer notes are excluded. Unknown amounts stay unknown.
A stopped node, divergent chain anchors, stale syncs or unsupported conditional
on-chain states must still prevent certification by the consumer. Atomicity does
not itself establish completeness or authorize trading.

The pinned Kaleido rust-lightning accounting extension is required. Applications
must carry the root Cargo patch for upstream rust-lightning transitive dependencies,
including payment-instructions, to avoid mixing incompatible Lightning types.
