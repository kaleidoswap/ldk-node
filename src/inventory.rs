// This file is Copyright its original authors, visible in version control history.
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. You may not use this file except in
// accordance with one or both of these licenses.

//! Read-only financial evidence. No aggregate ownership or atomicity is inferred.

/// Read-only InventoryTip evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryTip {
	/// Block hash in display order.
	pub hash: String,
	/// Block height.
	pub height: u32,
}

/// Read-only InventoryOutput evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryOutput {
	/// Transaction ID in display order.
	pub txid: String,
	/// Output index.
	pub vout: u32,
	/// Output amount in satoshis.
	pub value_sat: u64,
	/// Hex encoded locking script.
	pub script_pubkey: String,
}

/// Read-only InventoryUtxo evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryUtxo {
	/// Wallet-owned unspent output.
	pub output: InventoryOutput,
	/// Direct or transitive confirmation anchor; absent for unconfirmed outputs.
	pub confirmation: Option<InventoryTip>,
	/// Descendant transaction proving confirmation, if direct confirmation is unknown.
	pub transitively: Option<String>,
	/// First mempool observation, UNIX seconds.
	pub first_seen: Option<u64>,
	/// Latest mempool observation, UNIX seconds.
	pub last_seen: Option<u64>,
}

/// Read-only WalletInventory evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WalletInventory {
	/// Wallet chain anchor collected under the UTXO lock.
	pub tip: InventoryTip,
	/// All wallet unspent outputs; not necessarily spendable.
	pub utxos: Vec<InventoryUtxo>,
}

/// Read-only InventoryHtlc evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryHtlc {
	/// True for an incoming HTLC.
	pub inbound: bool,
	/// Channel-local HTLC identifier; absent before assignment.
	pub htlc_id: Option<u64>,
	/// Conditional amount in millisatoshis, not unconditional ownership.
	pub amount_msat: u64,
	/// Payment hash only, never its preimage.
	pub payment_hash: String,
	/// Absolute expiry height.
	pub cltv_expiry: u32,
	/// LDK HTLC state; absent when unavailable.
	pub state: Option<String>,
	/// Whether the HTLC is trimmed on commitment.
	pub is_dust: bool,
	/// Outbound skimmed fee, when available.
	pub skimmed_fee_msat: Option<u64>,
}

/// Read-only InventoryChannel evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryChannel {
	/// Channel identifier.
	pub channel_id: String,
	/// Peer public key.
	pub counterparty_node_id: String,
	/// Current funding output; includes the peer share and is not our owned balance.
	pub funding: Option<InventoryOutput>,
	/// Pending conditional transfers.
	pub htlcs: Vec<InventoryHtlc>,
	/// Exact pre-fee holder allocation; includes unresolved outbound encumbrances.
	pub accounting_balance_msat: Option<u64>,
}

/// Read-only InventoryCandidate evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryCandidate {
	/// Claimable amount excluding commitment fee.
	pub amount_sat: u64,
	/// Commitment fee, including applicable dust and rounding.
	pub transaction_fee_sat: u64,
}

/// Read-only InventoryClaim evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryClaim {
	/// LDK claim category; conditional categories must remain conditional.
	pub kind: String,
	/// Claim amount when not represented by candidates.
	pub amount_sat: Option<u64>,
	/// Category-specific confirmation, timeout, claimable or expiry height.
	pub height: Option<u32>,
	/// HTLC payment hash, never its preimage.
	pub payment_hash: Option<String>,
	/// Whether timeout claim originated as a payment rather than forwarding.
	pub outbound_payment: Option<bool>,
	/// Close or HTLC source for confirmation claims.
	pub source: Option<String>,
	/// All alternative commitments; never sum candidates.
	pub candidates: Vec<InventoryCandidate>,
	/// LDK confirmed candidate index; zero does not select the latest splice.
	pub confirmed_candidate_index: Option<u64>,
	/// LDK outbound payment rounding component.
	pub outbound_payment_rounded_msat: Option<u64>,
	/// LDK forwarded rounding component.
	pub outbound_forwarded_rounded_msat: Option<u64>,
	/// LDK inbound claiming rounding component.
	pub inbound_claiming_rounded_msat: Option<u64>,
	/// LDK unclaimed inbound rounding component.
	pub inbound_rounded_msat: Option<u64>,
}

/// Read-only InventoryMonitor evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryMonitor {
	/// Monitored channel identifier.
	pub channel_id: String,
	/// Monitored funding transaction ID.
	pub funding_txid: String,
	/// Monitored funding output index.
	pub funding_vout: u32,
	/// Monitor chain anchor.
	pub tip: InventoryTip,
	/// Claim evidence; does not assert accounting ownership.
	pub claims: Vec<InventoryClaim>,
}

/// Read-only InventorySweep evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventorySweep {
	/// Original output being swept, for overlap detection.
	pub output: InventoryOutput,
	/// Originating channel when known.
	pub channel_id: Option<String>,
	/// pending_broadcast, pending_confirmation or confirmed.
	pub state: String,
	/// Latest spending transaction ID, if any.
	pub spending_txid: Option<String>,
	/// Spending transaction confirmation, if any.
	pub confirmation: Option<InventoryTip>,
	/// Earliest broadcast height, if delayed.
	pub delayed_until_height: Option<u32>,
}

/// Read-only FinancialInventory evidence.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FinancialInventory {
	/// Schema 2 for simultaneous snapshots; schema 1 for fallback evidence.
	pub schema_version: u32,
	/// Node public key.
	pub node_id: String,
	/// Bitcoin network name.
	pub network: String,
	/// Collection start time, UNIX milliseconds.
	pub started_at_ms: u64,
	/// Collection end time, UNIX milliseconds.
	pub finished_at_ms: u64,
	/// Channel-manager anchor before collection.
	pub node_tip_before: InventoryTip,
	/// Channel-manager anchor after collection.
	pub node_tip_after: InventoryTip,
	/// Wallet evidence collected under one lock.
	pub wallet: WalletInventory,
	/// Channel and pending HTLC evidence.
	pub channels: Vec<InventoryChannel>,
	/// Monitor claims and anchors.
	pub monitors: Vec<InventoryMonitor>,
	/// Output sweeper chain anchor.
	pub sweeper_tip: InventoryTip,
	/// Tracked output sweep evidence.
	pub sweeps: Vec<InventorySweep>,
	/// Explicit observation failures or changes. Empty does not imply atomicity.
	pub gaps: Vec<String>,
	/// True only when all financial component locks are held simultaneously.
	pub atomic: bool,
	/// Sanitized payment history captured under the same lock boundary (schema 2).
	pub payments: Vec<InventoryPayment>,
	/// Last successful on-chain sync, UNIX seconds.
	pub latest_wallet_sync: Option<u64>,
	/// Last successful Lightning sync, UNIX seconds.
	pub latest_lightning_sync: Option<u64>,
}
use std::time::{SystemTime, UNIX_EPOCH};

use bitcoin::{OutPoint, TxOut};
use lightning::chain::channelmonitor::Balance;
use lightning::util::sweep::{OutputSpendStatus, TrackedSpendableOutput};

impl InventoryOutput {
	pub(crate) fn new(outpoint: OutPoint, output: &TxOut) -> Self {
		Self {
			txid: outpoint.txid.to_string(),
			vout: outpoint.vout,
			value_sat: output.value.to_sat(),
			script_pubkey: output.script_pubkey.to_hex_string(),
		}
	}
}
impl From<lightning::chain::BestBlock> for InventoryTip {
	fn from(v: lightning::chain::BestBlock) -> Self {
		Self { hash: v.block_hash.to_string(), height: v.height }
	}
}
fn claim(balance: Balance) -> InventoryClaim {
	let mut c = InventoryClaim {
		kind: String::new(),
		amount_sat: None,
		height: None,
		payment_hash: None,
		outbound_payment: None,
		source: None,
		candidates: vec![],
		confirmed_candidate_index: None,
		outbound_payment_rounded_msat: None,
		outbound_forwarded_rounded_msat: None,
		inbound_claiming_rounded_msat: None,
		inbound_rounded_msat: None,
	};
	match balance {
		Balance::ClaimableOnChannelClose {
			balance_candidates,
			confirmed_balance_candidate_index,
			outbound_payment_htlc_rounded_msat,
			outbound_forwarded_htlc_rounded_msat,
			inbound_claiming_htlc_rounded_msat,
			inbound_htlc_rounded_msat,
		} => {
			c.kind = "claimable_on_channel_close".into();
			c.candidates = balance_candidates
				.into_iter()
				.map(|b| InventoryCandidate {
					amount_sat: b.amount_satoshis,
					transaction_fee_sat: b.transaction_fee_satoshis,
				})
				.collect();
			c.confirmed_candidate_index = Some(confirmed_balance_candidate_index as u64);
			c.outbound_payment_rounded_msat = Some(outbound_payment_htlc_rounded_msat);
			c.outbound_forwarded_rounded_msat = Some(outbound_forwarded_htlc_rounded_msat);
			c.inbound_claiming_rounded_msat = Some(inbound_claiming_htlc_rounded_msat);
			c.inbound_rounded_msat = Some(inbound_htlc_rounded_msat);
		},
		Balance::ClaimableAwaitingConfirmations {
			amount_satoshis,
			confirmation_height,
			source,
		} => {
			c.kind = "claimable_awaiting_confirmations".into();
			c.amount_sat = Some(amount_satoshis);
			c.height = Some(confirmation_height);
			c.source = Some(format!("{source:?}"));
		},
		Balance::ContentiousClaimable {
			amount_satoshis,
			timeout_height,
			payment_hash,
			payment_preimage: _,
		} => {
			c.kind = "contentious_claimable".into();
			c.amount_sat = Some(amount_satoshis);
			c.height = Some(timeout_height);
			c.payment_hash = Some(payment_hash.to_string());
		},
		Balance::MaybeTimeoutClaimableHTLC {
			amount_satoshis,
			claimable_height,
			payment_hash,
			outbound_payment,
		} => {
			c.kind = "maybe_timeout_claimable_htlc".into();
			c.amount_sat = Some(amount_satoshis);
			c.height = Some(claimable_height);
			c.payment_hash = Some(payment_hash.to_string());
			c.outbound_payment = Some(outbound_payment);
		},
		Balance::MaybePreimageClaimableHTLC { amount_satoshis, expiry_height, payment_hash } => {
			c.kind = "maybe_preimage_claimable_htlc".into();
			c.amount_sat = Some(amount_satoshis);
			c.height = Some(expiry_height);
			c.payment_hash = Some(payment_hash.to_string());
		},
		Balance::CounterpartyRevokedOutputClaimable { amount_satoshis } => {
			c.kind = "counterparty_revoked_output_claimable".into();
			c.amount_sat = Some(amount_satoshis);
		},
	}
	c
}
fn sweep(v: TrackedSpendableOutput) -> InventorySweep {
	use lightning::sign::SpendableOutputDescriptor;
	let output = match &v.descriptor {
		SpendableOutputDescriptor::StaticOutput { output, .. } => output,
		SpendableOutputDescriptor::DelayedPaymentOutput(d) => &d.output,
		SpendableOutputDescriptor::StaticPaymentOutput(d) => &d.output,
	};
	let mut result = InventorySweep {
		output: InventoryOutput::new(
			v.descriptor.spendable_outpoint().into_bitcoin_outpoint(),
			output,
		),
		channel_id: v.channel_id.map(|c| c.to_string()),
		state: String::new(),
		spending_txid: None,
		confirmation: None,
		delayed_until_height: None,
	};
	match v.status {
		OutputSpendStatus::PendingInitialBroadcast { delayed_until_height } => {
			result.state = "pending_broadcast".into();
			result.delayed_until_height = delayed_until_height;
		},
		OutputSpendStatus::PendingFirstConfirmation { latest_spending_tx, .. } => {
			result.state = "pending_confirmation".into();
			result.spending_txid = Some(latest_spending_tx.compute_txid().to_string());
		},
		OutputSpendStatus::PendingThresholdConfirmations {
			latest_spending_tx,
			confirmation_hash,
			confirmation_height,
			..
		} => {
			result.state = "confirmed".into();
			result.spending_txid = Some(latest_spending_tx.compute_txid().to_string());
			result.confirmation = Some(InventoryTip {
				hash: confirmation_hash.to_string(),
				height: confirmation_height,
			});
		},
	}
	result
}
fn now_ms() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}
impl crate::Node {
	/// Collects read-only evidence without claiming a globally atomic ownership snapshot.
	///
	/// Each component is read independently. Missing monitors, changing evidence and stale
	/// anchors are explicit gaps. Empty gaps do not establish accounting completeness.
	/// This Rust API does not alter wallet, payment or channel state.
	fn uncoordinated_inventory(&self) -> FinancialInventory {
		let started_at_ms = now_ms();
		let status = self.status();
		let node_tip_before = self.channel_manager.current_best_block().into();
		let wallet = self.wallet.inventory();
		let channels_before = self.channel_manager.list_channels();
		let mut channels: Vec<_> =
			channels_before.iter().map(|c| project_channel(c, None)).collect();
		channels.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
		let mut gaps = Vec::new();
		let mut monitor_ids = self.chain_monitor.list_monitors();
		monitor_ids.sort();
		let mut monitors = Vec::new();
		let mut observed_monitor_ids = Vec::new();
		for id in &monitor_ids {
			match self.chain_monitor.get_monitor(*id) {
				Ok(m) => {
					let funding = m.get_funding_txo();
					observed_monitor_ids.push(*id);
					monitors.push(InventoryMonitor {
						channel_id: id.to_string(),
						funding_txid: funding.txid.to_string(),
						funding_vout: funding.index as u32,
						tip: m.current_best_block().into(),
						claims: m.get_claimable_balances().into_iter().map(claim).collect(),
					});
				},
				Err(()) => gaps.push(format!("monitor_unavailable:{id}")),
			}
		}
		let sweeper_tip = self.output_sweeper.current_best_block().into();
		let swept_before = self.output_sweeper.tracked_spendable_outputs();
		let mut sweeps: Vec<_> = swept_before.iter().cloned().map(sweep).collect();
		sweeps
			.sort_by(|a, b| (&a.output.txid, a.output.vout).cmp(&(&b.output.txid, b.output.vout)));
		if self.wallet.inventory() != wallet {
			gaps.push("wallet_changed_during_collection".into());
		}
		if self.channel_manager.list_channels() != channels_before {
			gaps.push("channels_changed_during_collection".into());
		}
		let mut monitor_ids_after = self.chain_monitor.list_monitors();
		monitor_ids_after.sort();
		if monitor_ids_after != monitor_ids {
			gaps.push("monitor_set_changed_during_collection".into());
		}
		for (id, observed) in observed_monitor_ids.iter().zip(&monitors) {
			match self.chain_monitor.get_monitor(*id) {
				Ok(m)
					if InventoryTip::from(m.current_best_block()) == observed.tip
						&& m.get_funding_txo().txid.to_string() == observed.funding_txid
						&& u32::from(m.get_funding_txo().index) == observed.funding_vout
						&& m.get_claimable_balances()
							.into_iter()
							.map(claim)
							.collect::<Vec<_>>() == observed.claims => {},
				_ => gaps.push(format!("monitor_changed_during_collection:{id}")),
			}
		}
		if self.output_sweeper.tracked_spendable_outputs() != swept_before
			|| InventoryTip::from(self.output_sweeper.current_best_block()) != sweeper_tip
		{
			gaps.push("sweeper_changed_during_collection".into());
		}
		let node_tip_after = self.channel_manager.current_best_block().into();
		if node_tip_before != node_tip_after {
			gaps.push("node_tip_changed_during_collection".into());
		}
		if wallet.tip != node_tip_after
			|| sweeper_tip != node_tip_after
			|| monitors.iter().any(|m| m.tip != node_tip_after)
		{
			gaps.push("component_chain_anchors_differ".into());
		}
		if !status.is_running {
			gaps.push("node_not_running".into());
		}
		FinancialInventory {
			schema_version: 1,
			node_id: self.node_id().to_string(),
			network: status.network.to_string(),
			started_at_ms,
			finished_at_ms: now_ms(),
			node_tip_before,
			node_tip_after,
			wallet,
			channels,
			monitors,
			sweeper_tip,
			sweeps,
			gaps,
			atomic: false,
			payments: Vec::new(),
			latest_wallet_sync: status.latest_onchain_wallet_sync_timestamp,
			latest_lightning_sync: status.latest_lightning_wallet_sync_timestamp,
		}
	}
}

fn project_channel(
	c: &lightning::ln::channel_state::ChannelDetails, accounting_balance_msat: Option<u64>,
) -> InventoryChannel {
	let mut htlcs: Vec<_> = c
		.pending_inbound_htlcs
		.iter()
		.map(|h| InventoryHtlc {
			inbound: true,
			htlc_id: Some(h.htlc_id),
			amount_msat: h.amount_msat,
			payment_hash: h.payment_hash.to_string(),
			cltv_expiry: h.cltv_expiry,
			state: h.state.as_ref().map(|s| format!("{s:?}")),
			is_dust: h.is_dust,
			skimmed_fee_msat: None,
		})
		.collect();
	htlcs.extend(c.pending_outbound_htlcs.iter().map(|h| InventoryHtlc {
		inbound: false,
		htlc_id: h.htlc_id,
		amount_msat: h.amount_msat,
		payment_hash: h.payment_hash.to_string(),
		cltv_expiry: h.cltv_expiry,
		state: h.state.as_ref().map(|s| format!("{s:?}")),
		is_dust: h.is_dust,
		skimmed_fee_msat: h.skimmed_fee_msat,
	}));
	InventoryChannel {
		channel_id: c.channel_id.to_string(),
		counterparty_node_id: c.counterparty.node_id.to_string(),
		funding: c.funding_txo.and_then(|o| {
			c.get_funding_output().map(|out| InventoryOutput::new(o.into_bitcoin_outpoint(), &out))
		}),
		htlcs,
		accounting_balance_msat,
	}
}

/// Sanitized payment accounting evidence. Never contains secrets or preimages.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InventoryPayment {
	/// Stable node payment ID.
	pub payment_id: String,
	/// On-chain transaction ID, absent for Lightning.
	pub txid: Option<String>,
	/// Lightning payment hash, absent when not yet assigned or for on-chain.
	pub payment_hash: Option<String>,
	/// Whether value is received by this node.
	pub inbound: bool,
	/// pending, succeeded or failed.
	pub status: String,
	/// Actual amount, unknown remains absent.
	pub amount_msat: Option<u64>,
	/// Actual fee, unknown remains absent.
	pub fee_msat: Option<u64>,
	/// Last update in UNIX seconds.
	pub updated_at: u64,
}
fn project_payment(p: crate::payment::PaymentDetails) -> InventoryPayment {
	use crate::payment::{PaymentDirection, PaymentKind, PaymentStatus};
	let (txid, payment_hash) = match p.kind {
		PaymentKind::Onchain { txid, .. } => (Some(txid.to_string()), None),
		PaymentKind::Bolt11 { hash, .. }
		| PaymentKind::Bolt11Jit { hash, .. }
		| PaymentKind::Spontaneous { hash, .. } => (None, Some(hash.to_string())),
		PaymentKind::Bolt12Offer { hash, .. } | PaymentKind::Bolt12Refund { hash, .. } => {
			(None, hash.map(|h| h.to_string()))
		},
	};
	InventoryPayment {
		payment_id: crate::hex_utils::to_string(&p.id.0),
		txid,
		payment_hash,
		inbound: p.direction == PaymentDirection::Inbound,
		status: match p.status {
			PaymentStatus::Pending => "pending",
			PaymentStatus::Succeeded => "succeeded",
			PaymentStatus::Failed => "failed",
		}
		.into(),
		amount_msat: p.amount_msat,
		fee_msat: p.fee_paid_msat,
		updated_at: p.latest_update_timestamp,
	}
}
impl crate::Node {
	/// Captures wallet, channels, monitors, sweeper and payments under one lock
	/// boundary. Secondary locks are try-locks to avoid inversion with LDK writers.
	/// Contention returns explicitly non-atomic evidence, never a guessed snapshot.
	pub fn financial_inventory(&self) -> FinancialInventory {
		let started_at_ms = now_ms();
		let status = self.status();
		let result = self.wallet.with_inventory(|wallet| {
			self.channel_manager
				.try_with_accounting_snapshot(|tip, channels| {
					self.chain_monitor
						.try_with_accounting_snapshot(|monitors| {
							self.output_sweeper
								.try_with_accounting_snapshot(|sweeper_tip, sweeps| {
									self.payment_store.try_with_objects(|payments| {
										let mut channels: Vec<_> = channels
											.iter()
											.map(|(c, balance)| project_channel(c, Some(*balance)))
											.collect();
										channels.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
										let mut monitors: Vec<_> = monitors
											.into_iter()
											.map(|(id, m)| {
												let funding = m.get_funding_txo();
												InventoryMonitor {
													channel_id: id.to_string(),
													funding_txid: funding.txid.to_string(),
													funding_vout: u32::from(funding.index),
													tip: m.current_best_block().into(),
													claims: m
														.get_claimable_balances()
														.into_iter()
														.map(claim)
														.collect(),
												}
											})
											.collect();
										monitors.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
										let mut sweeps: Vec<_> =
											sweeps.into_iter().map(sweep).collect();
										sweeps.sort_by(|a, b| {
											(&a.output.txid, a.output.vout)
												.cmp(&(&b.output.txid, b.output.vout))
										});
										let mut payments: Vec<_> =
											payments.into_iter().map(project_payment).collect();
										payments.sort_by(|a, b| a.payment_id.cmp(&b.payment_id));
										FinancialInventory {
											schema_version: 2,
											node_id: self.node_id().to_string(),
											network: status.network.to_string(),
											started_at_ms,
											finished_at_ms: now_ms(),
											node_tip_before: tip.into(),
											node_tip_after: tip.into(),
											wallet,
											channels,
											monitors,
											sweeper_tip: sweeper_tip.into(),
											sweeps,
											payments,
											atomic: true,
											gaps: if status.is_running {
												vec![]
											} else {
												vec!["node_not_running".into()]
											},
											latest_wallet_sync: status
												.latest_onchain_wallet_sync_timestamp,
											latest_lightning_sync: status
												.latest_lightning_wallet_sync_timestamp,
										}
									})
								})
								.flatten()
						})
						.flatten()
				})
				.flatten()
		});
		result.unwrap_or_else(|| {
			let mut inventory = self.uncoordinated_inventory();
			inventory.gaps.push("accounting_snapshot_contended".into());
			inventory
		})
	}
}

#[cfg(test)]
mod tests {
	use bitcoin::hashes::Hash;
	use lightning::chain::channelmonitor::HolderCommitmentTransactionBalance;
	use lightning_types::payment::{PaymentHash, PaymentPreimage};

	use super::*;
	#[test]
	fn contentious_claim_never_exposes_preimage() {
		let projected = claim(Balance::ContentiousClaimable {
			amount_satoshis: 123,
			timeout_height: 45,
			payment_hash: PaymentHash([1; 32]),
			payment_preimage: PaymentPreimage([42; 32]),
		});
		let json = serde_json::to_string(&projected).unwrap();
		assert!(!json.contains("preimage"));
		assert!(!json.contains(&"2a".repeat(32)));
		assert_eq!(projected.kind, "contentious_claimable");
		assert_eq!(projected.amount_sat, Some(123));
	}
	#[test]
	fn alternative_commitments_and_rounding_are_preserved() {
		let projected = claim(Balance::ClaimableOnChannelClose {
			balance_candidates: vec![
				HolderCommitmentTransactionBalance {
					amount_satoshis: 100,
					transaction_fee_satoshis: 2,
				},
				HolderCommitmentTransactionBalance {
					amount_satoshis: 200,
					transaction_fee_satoshis: 3,
				},
			],
			confirmed_balance_candidate_index: 0,
			outbound_payment_htlc_rounded_msat: 11,
			outbound_forwarded_htlc_rounded_msat: 12,
			inbound_claiming_htlc_rounded_msat: 13,
			inbound_htlc_rounded_msat: 14,
		});
		assert_eq!(projected.amount_sat, None);
		assert_eq!(projected.candidates.len(), 2);
		assert_eq!(projected.confirmed_candidate_index, Some(0));
		assert_eq!(projected.inbound_rounded_msat, Some(14));
	}
	#[test]
	fn sweep_keeps_source_outpoint_for_deduplication() {
		let output = bitcoin::TxOut {
			value: bitcoin::Amount::from_sat(400),
			script_pubkey: bitcoin::ScriptBuf::new(),
		};
		let outpoint =
			lightning::chain::transaction::OutPoint { txid: bitcoin::Txid::all_zeros(), index: 2 };
		let projected = sweep(TrackedSpendableOutput {
			descriptor: lightning::sign::SpendableOutputDescriptor::StaticOutput {
				outpoint,
				output,
				channel_keys_id: None,
			},
			channel_id: None,
			counterparty_node_id: None,
			status: OutputSpendStatus::PendingInitialBroadcast { delayed_until_height: Some(9) },
		});
		assert_eq!(projected.output.vout, 2);
		assert_eq!(projected.output.value_sat, 400);
		assert_eq!(projected.delayed_until_height, Some(9));
		assert_eq!(projected.spending_txid, None);
	}
	#[test]
	fn stopped_node_is_explicitly_incomplete_and_wallet_read_is_repeatable() {
		let dir =
			std::env::temp_dir().join(format!("ldk-inventory-{}-{}", std::process::id(), now_ms()));
		let config = crate::config::Config {
			network: bitcoin::Network::Regtest,
			storage_dir_path: dir.to_string_lossy().into_owned(),
			..Default::default()
		};
		let builder = crate::Builder::from_config(config);
		let entropy = crate::entropy::NodeEntropy::from_bip39_mnemonic(
			crate::entropy::generate_entropy_mnemonic(None),
			None,
		);
		let node = builder.build(entropy.into()).unwrap();
		let first = node.financial_inventory();
		let second = node.financial_inventory();
		assert_eq!(first.wallet, second.wallet);
		assert!(first.wallet.utxos.is_empty());
		assert_eq!(first.network, "regtest");
		assert!(first.gaps.contains(&"node_not_running".to_owned()));
		assert!(first.atomic);
		assert_eq!(first.schema_version, 2);
		let contended =
			node.payment_store.try_with_objects(|_| node.financial_inventory()).unwrap();
		assert!(!contended.atomic);
		assert!(contended.gaps.contains(&"accounting_snapshot_contended".to_owned()));
		assert!(first.finished_at_ms >= first.started_at_ms);
		drop(node);
		if dir.exists() {
			std::fs::remove_dir_all(dir).unwrap();
		}
	}
}
