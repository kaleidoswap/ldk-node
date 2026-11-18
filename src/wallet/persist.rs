// This file is Copyright its original authors, visible in version control history.
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. You may not use this file except in
// accordance with one or both of these licenses.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use bdk_chain::Merge;
use bdk_wallet::{AsyncWalletPersister, ChangeSet};
use lightning::impl_writeable_tlv_based;
use lightning::util::persist::KVStore;
use lightning::util::ser::{Readable, Writeable};

use crate::io::utils::{
	read_bdk_wallet_change_set, write_bdk_wallet_change_descriptor, write_bdk_wallet_descriptor,
	write_bdk_wallet_indexer, write_bdk_wallet_local_chain, write_bdk_wallet_network,
	write_bdk_wallet_tx_graph,
};
use crate::io::{
	BDK_WALLET_ADDRESS_POOL_KEY, BDK_WALLET_ADDRESS_POOL_PRIMARY_NAMESPACE,
	BDK_WALLET_ADDRESS_POOL_SECONDARY_NAMESPACE,
};
use crate::logger::{log_error, log_info, LdkLogger, Logger};
use crate::types::DynStore;

/// The persisted derivation indices of the wallet's address pool.
///
/// The record is advisory and reconstructible: it is written before the reveals it references
/// are necessarily persisted, so it may briefly lead the persisted [`ChangeSet::indexer`].
/// Readers must validate each index against the wallet's revealed range before handing out any
/// of the addresses; dropped entries are re-derived to the same indices by the next refill.
///
/// [`ChangeSet::indexer`]: bdk_wallet::ChangeSet::indexer
struct AddressPoolRecord {
	indices: Vec<u32>,
}

impl_writeable_tlv_based!(AddressPoolRecord, {
	(0, indices, required_vec),
});

/// How many blocks below the tip [`compact_local_chain`] keeps, regardless of whether a wallet
/// transaction confirmed in them, so that reorgs are still detected against a dense chain.
pub(crate) const LOCAL_CHAIN_RECENT_BLOCKS: u32 = 288;

/// Drops the local-chain checkpoints the wallet does not need, returning how many were dropped.
///
/// Every chain update adds the blocks it saw to BDK's local chain and nothing ever removes them,
/// so a long-lived wallet ends up with a checkpoint for nearly every block since its birthday.
/// BDK looks checkpoints up by walking that list from the tip, once per anchor whenever it
/// syncs or canonicalizes the transaction graph, which makes both scale with the product of the
/// wallet's transaction count and its age.
///
/// A transaction only counts as confirmed while the block its anchor points to is in the local
/// chain, so we keep the genesis block, the tip, the block of every anchor, and the last
/// [`LOCAL_CHAIN_RECENT_BLOCKS`] blocks.
pub(crate) fn compact_local_chain(change_set: &mut ChangeSet) -> usize {
	let blocks = &mut change_set.local_chain.blocks;
	let Some(tip_height) = blocks.iter().rev().find(|(_, hash)| hash.is_some()).map(|(h, _)| *h)
	else {
		return 0;
	};
	let recent_from = tip_height.saturating_sub(LOCAL_CHAIN_RECENT_BLOCKS);
	let anchor_heights: BTreeSet<u32> =
		change_set.tx_graph.anchors.iter().map(|(anchor, _)| anchor.block_id.height).collect();

	let before = blocks.len();
	// `None` entries only record removals and never add a checkpoint on load.
	blocks.retain(|height, hash| {
		hash.is_some()
			&& (*height == 0 || *height >= recent_from || anchor_heights.contains(height))
	});
	before - blocks.len()
}

pub(crate) struct KVStoreWalletPersister {
	latest_change_set: Option<ChangeSet>,
	pending_change_set: ChangeSet,
	kv_store: Arc<DynStore>,
	logger: Arc<Logger>,
}

impl KVStoreWalletPersister {
	pub(crate) fn new(kv_store: Arc<DynStore>, logger: Arc<Logger>) -> Self {
		Self { latest_change_set: None, pending_change_set: ChangeSet::default(), kv_store, logger }
	}

	async fn initialize_inner(&mut self) -> Result<ChangeSet, std::io::Error> {
		// Return immediately if we have already been initialized.
		if let Some(latest_change_set) = self.latest_change_set.as_ref() {
			return Ok(latest_change_set.clone());
		}

		let change_set_opt = read_bdk_wallet_change_set(&*self.kv_store, &*self.logger).await?;

		let mut change_set = match change_set_opt {
			Some(persisted_change_set) => persisted_change_set,
			None => {
				// BDK docs state: "The implementation must return all data currently stored in the
				// persister. If there is no data, return an empty changeset (using
				// ChangeSet::default())."
				ChangeSet::default()
			},
		};
		let removed = compact_local_chain(&mut change_set);
		if removed > 0 {
			log_info!(
				self.logger,
				"Compacted the on-chain wallet's local chain: dropped {} checkpoints, kept {}.",
				removed,
				change_set.local_chain.blocks.len()
			);
			// The compacted chain is complete on its own, so failing to write it back only
			// leaves the larger one in storage until the next chain update rewrites it.
			if let Err(e) = write_bdk_wallet_local_chain(
				&change_set.local_chain,
				&*self.kv_store,
				Arc::clone(&self.logger),
			)
			.await
			{
				log_error!(self.logger, "Failed to persist the compacted local chain: {}", e);
			}
		}

		self.latest_change_set = Some(change_set.clone());
		Ok(change_set)
	}

	async fn persist_inner(
		latest_change_set_opt: &mut Option<ChangeSet>, kv_store: &Arc<DynStore>,
		logger: &Arc<Logger>, change_set: &ChangeSet,
	) -> Result<(), std::io::Error> {
		if change_set.is_empty() {
			return Ok(());
		}

		let kv_store = kv_store.as_ref();

		// We're allowed to fail here if we're not initialized, BDK docs state: "This method can fail if the
		// persister is not initialized."
		let latest_change_set = latest_change_set_opt.as_mut().ok_or_else(|| {
			std::io::Error::new(
				std::io::ErrorKind::Other,
				"Wallet must be initialized before calling persist",
			)
		})?;

		// Check that we'd never accidentally override any persisted data if the change set doesn't
		// match our descriptor/change_descriptor/network.
		if let Some(descriptor) = change_set.descriptor.as_ref() {
			if latest_change_set.descriptor.is_some()
				&& latest_change_set.descriptor.as_ref() != Some(descriptor)
			{
				debug_assert!(false, "Wallet descriptor must never change");
				log_error!(
					logger,
					"Wallet change set doesn't match persisted descriptor. This should never happen."
				);
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Wallet change set doesn't match persisted descriptor. This should never happen."
				));
			} else {
				latest_change_set.descriptor = Some(descriptor.clone());
				write_bdk_wallet_descriptor(&descriptor, &*kv_store, Arc::clone(&logger)).await?;
			}
		}

		if let Some(change_descriptor) = change_set.change_descriptor.as_ref() {
			if latest_change_set.change_descriptor.is_some()
				&& latest_change_set.change_descriptor.as_ref() != Some(change_descriptor)
			{
				debug_assert!(false, "Wallet change_descriptor must never change");
				log_error!(
					logger,
					"Wallet change set doesn't match persisted change_descriptor. This should never happen."
				);
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Wallet change set doesn't match persisted change_descriptor. This should never happen."
				));
			} else {
				latest_change_set.change_descriptor = Some(change_descriptor.clone());
				write_bdk_wallet_change_descriptor(
					&change_descriptor,
					&*kv_store,
					Arc::clone(&logger),
				)
				.await?;
			}
		}

		if let Some(network) = change_set.network {
			if latest_change_set.network.is_some() && latest_change_set.network != Some(network) {
				debug_assert!(false, "Wallet network must never change");
				log_error!(
					logger,
					"Wallet change set doesn't match persisted network. This should never happen."
				);
				return Err(std::io::Error::new(
					std::io::ErrorKind::InvalidData,
					"Wallet change set doesn't match persisted network. This should never happen.",
				));
			} else {
				latest_change_set.network = Some(network);
				write_bdk_wallet_network(&network, &*kv_store, Arc::clone(&logger)).await?;
			}
		}

		debug_assert!(
			latest_change_set.descriptor.is_some()
				&& latest_change_set.change_descriptor.is_some()
				&& latest_change_set.network.is_some(),
			"descriptor, change_descriptor, and network are mandatory ChangeSet fields"
		);

		// Merge and persist the sub-changesets individually if necessary.
		//
		// According to the BDK team the individual sub-changesets can be persisted
		// individually/non-atomically, "(h)owever, the localchain tip is used by block-by-block
		// chain sources as a reference as to where to sync from, so I would persist that last", "I
		// would write in this order: indexer, tx_graph, local_chain", which is why we follow this
		// particular order.
		if !change_set.indexer.is_empty() {
			latest_change_set.indexer.merge(change_set.indexer.clone());
			write_bdk_wallet_indexer(&latest_change_set.indexer, &*kv_store, Arc::clone(&logger))
				.await?;
		}

		if !change_set.tx_graph.is_empty() {
			latest_change_set.tx_graph.merge(change_set.tx_graph.clone());
			write_bdk_wallet_tx_graph(&latest_change_set.tx_graph, &*kv_store, Arc::clone(&logger))
				.await?;
		}

		if !change_set.local_chain.is_empty() {
			latest_change_set.local_chain.merge(change_set.local_chain.clone());
			write_bdk_wallet_local_chain(
				&latest_change_set.local_chain,
				&*kv_store,
				Arc::clone(&logger),
			)
			.await?;
		}

		Ok(())
	}

	pub(super) async fn persist_changeset(
		&mut self, change_set: ChangeSet,
	) -> Result<(), std::io::Error> {
		self.stage(change_set);
		self.persist_staged().await
	}

	/// Merges the given change set into the pending one without persisting it, to be flushed by
	/// [`Self::persist_staged`] or any later persist call.
	///
	/// Staging is synchronous, letting callers that run as abortable tasks hand a change set
	/// over without an intervening await point: once staged, an abort leaves it pending for the
	/// next persist rather than dropping it.
	pub(super) fn stage(&mut self, change_set: ChangeSet) {
		self.pending_change_set.merge(change_set);
	}

	/// Persists the pending change set, retaining it for retry on failure.
	pub(super) async fn persist_staged(&mut self) -> Result<(), std::io::Error> {
		Self::persist_inner(
			&mut self.latest_change_set,
			&self.kv_store,
			&self.logger,
			&self.pending_change_set,
		)
		.await?;
		let _ = std::mem::take(&mut self.pending_change_set);
		Ok(())
	}

	/// Persists the address-pool derivation indices.
	///
	/// See [`AddressPoolRecord`] for what readers may assume about the persisted indices.
	pub(super) async fn persist_address_pool(
		&mut self, indices: Vec<u32>,
	) -> Result<(), std::io::Error> {
		let record = AddressPoolRecord { indices };
		KVStore::write(
			&*self.kv_store,
			BDK_WALLET_ADDRESS_POOL_PRIMARY_NAMESPACE,
			BDK_WALLET_ADDRESS_POOL_SECONDARY_NAMESPACE,
			BDK_WALLET_ADDRESS_POOL_KEY,
			record.encode(),
		)
		.await
		.map_err(|e| {
			log_error!(self.logger, "Failed to persist address pool: {}", e);
			e.into()
		})
	}
}

/// Reads the persisted address-pool derivation indices, or an empty list if none were
/// persisted yet.
pub(crate) async fn read_address_pool(
	kv_store: &DynStore, logger: &Logger,
) -> Result<Vec<u32>, std::io::Error> {
	let reader = match KVStore::read(
		kv_store,
		BDK_WALLET_ADDRESS_POOL_PRIMARY_NAMESPACE,
		BDK_WALLET_ADDRESS_POOL_SECONDARY_NAMESPACE,
		BDK_WALLET_ADDRESS_POOL_KEY,
	)
	.await
	{
		Ok(reader) => reader,
		Err(e) if e.kind() == lightning::io::ErrorKind::NotFound => return Ok(Vec::new()),
		Err(e) => return Err(e.into()),
	};
	let record = match AddressPoolRecord::read(&mut &*reader) {
		Ok(record) => record,
		Err(e) => {
			// The record is a reconstructible cache: a decode failure (corruption, or a
			// future version's incompatible encoding) at worst costs the pool's indices, so
			// degrade to an empty pool rather than failing the node's startup.
			log_error!(logger, "Dropping undecodable address pool: {}", e);
			return Ok(Vec::new());
		},
	};
	Ok(record.indices)
}

impl AsyncWalletPersister for KVStoreWalletPersister {
	type Error = std::io::Error;

	fn initialize<'a>(
		persister: &'a mut Self,
	) -> Pin<Box<dyn Future<Output = Result<ChangeSet, Self::Error>> + Send + 'a>>
	where
		Self: 'a,
	{
		Box::pin(persister.initialize_inner())
	}

	fn persist<'a>(
		persister: &'a mut Self, change_set: &'a ChangeSet,
	) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + 'a>>
	where
		Self: 'a,
	{
		Box::pin(Self::persist_inner(
			&mut persister.latest_change_set,
			&persister.kv_store,
			&persister.logger,
			change_set,
		))
	}
}

#[cfg(test)]
mod tests {
	use std::future::Future;
	use std::sync::Arc;
	use std::time::Duration;

	use bdk_wallet::{AsyncWalletPersister, ChangeSet, Wallet as BdkWallet};
	use bitcoin::Network;
	use lightning::io;
	use lightning::util::persist::{KVStore, PageToken, PaginatedKVStore, PaginatedListResponse};

	use super::KVStoreWalletPersister;
	use crate::io::test_utils::InMemoryStore;
	use crate::logger::Logger;
	use crate::types::{DynStore, DynStoreWrapper};

	const EXTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
	const INTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

	#[derive(Clone)]
	struct GatedStore {
		inner: Arc<InMemoryStore>,
		write_gate: Arc<tokio::sync::RwLock<()>>,
	}

	impl KVStore for GatedStore {
		fn read(
			&self, primary_namespace: &str, secondary_namespace: &str, key: &str,
		) -> impl Future<Output = Result<Vec<u8>, io::Error>> + 'static + Send {
			KVStore::read(&*self.inner, primary_namespace, secondary_namespace, key)
		}

		fn write(
			&self, primary_namespace: &str, secondary_namespace: &str, key: &str, buf: Vec<u8>,
		) -> impl Future<Output = Result<(), io::Error>> + 'static + Send {
			let inner = Arc::clone(&self.inner);
			let write_gate = Arc::clone(&self.write_gate);
			let primary_namespace = primary_namespace.to_string();
			let secondary_namespace = secondary_namespace.to_string();
			let key = key.to_string();
			async move {
				let _guard = write_gate.read().await;
				KVStore::write(&*inner, &primary_namespace, &secondary_namespace, &key, buf).await
			}
		}

		fn remove(
			&self, primary_namespace: &str, secondary_namespace: &str, key: &str, lazy: bool,
		) -> impl Future<Output = Result<(), io::Error>> + 'static + Send {
			KVStore::remove(&*self.inner, primary_namespace, secondary_namespace, key, lazy)
		}

		fn list(
			&self, primary_namespace: &str, secondary_namespace: &str,
		) -> impl Future<Output = Result<Vec<String>, io::Error>> + 'static + Send {
			KVStore::list(&*self.inner, primary_namespace, secondary_namespace)
		}
	}

	impl PaginatedKVStore for GatedStore {
		fn list_paginated(
			&self, primary_namespace: &str, secondary_namespace: &str,
			page_token: Option<PageToken>,
		) -> impl Future<Output = Result<PaginatedListResponse, io::Error>> + 'static + Send {
			PaginatedKVStore::list_paginated(
				&*self.inner,
				primary_namespace,
				secondary_namespace,
				page_token,
			)
		}
	}

	#[tokio::test]
	async fn retains_pending_changes_when_persist_is_cancelled() {
		let gated_store = GatedStore {
			inner: Arc::new(InMemoryStore::new()),
			write_gate: Arc::new(tokio::sync::RwLock::new(())),
		};
		let store: Arc<DynStore> = Arc::new(DynStoreWrapper(gated_store.clone()));
		let logger = Arc::new(Logger::new_log_facade());
		let mut persister = KVStoreWalletPersister::new(Arc::clone(&store), Arc::clone(&logger));
		AsyncWalletPersister::initialize(&mut persister).await.unwrap();

		let mut wallet = BdkWallet::create(EXTERNAL_DESCRIPTOR, INTERNAL_DESCRIPTOR)
			.network(Network::Regtest)
			.create_wallet_no_persist()
			.unwrap();
		let change_set = wallet.take_staged().unwrap();

		let gate_guard = gated_store.write_gate.write().await;
		{
			let persist_fut = persister.persist_changeset(change_set);
			tokio::pin!(persist_fut);
			let poll_res = tokio::time::timeout(Duration::from_millis(100), &mut persist_fut).await;
			assert!(poll_res.is_err(), "persist should be parked on the gated store write");
		}
		drop(gate_guard);

		persister.persist_changeset(ChangeSet::default()).await.unwrap();

		let mut reloaded_persister = KVStoreWalletPersister::new(store, logger);
		let reloaded = AsyncWalletPersister::initialize(&mut reloaded_persister).await.unwrap();
		assert_eq!(reloaded.network, Some(Network::Regtest));
	}

	#[tokio::test]
	async fn retries_changes_after_persistence_failure() {
		let store: Arc<DynStore> = Arc::new(DynStoreWrapper(InMemoryStore::new()));
		let logger = Arc::new(Logger::new_log_facade());
		let mut persister = KVStoreWalletPersister::new(Arc::clone(&store), Arc::clone(&logger));
		let mut wallet = BdkWallet::create(EXTERNAL_DESCRIPTOR, INTERNAL_DESCRIPTOR)
			.network(Network::Regtest)
			.create_wallet_no_persist()
			.unwrap();
		let change_set = wallet.take_staged().unwrap();

		assert!(persister.persist_changeset(change_set).await.is_err());
		AsyncWalletPersister::initialize(&mut persister).await.unwrap();
		persister.persist_changeset(ChangeSet::default()).await.unwrap();

		let mut reloaded_persister = KVStoreWalletPersister::new(store, logger);
		let reloaded = AsyncWalletPersister::initialize(&mut reloaded_persister).await.unwrap();
		assert_eq!(reloaded.network, Some(Network::Regtest));
	}
}

#[cfg(test)]
mod compaction_tests {
	use std::collections::BTreeMap;
	use std::sync::Arc;
	use std::time::Instant;

	use bdk_chain::local_chain::LocalChain;
	use bdk_chain::{BlockId, CanonicalizationParams, ConfirmationBlockTime, Merge, TxGraph};
	use bdk_wallet::{AsyncWalletPersister, ChangeSet, Wallet as BdkWallet};
	use bitcoin::hashes::Hash;
	use bitcoin::{BlockHash, Network, Txid};
	use lightning::util::persist::KVStore;

	use super::{compact_local_chain, KVStoreWalletPersister, LOCAL_CHAIN_RECENT_BLOCKS};
	use crate::io::test_utils::InMemoryStore;
	use crate::io::utils::{read_bdk_wallet_local_chain, read_bdk_wallet_tx_graph};
	use crate::io::{
		BDK_WALLET_LOCAL_CHAIN_KEY, BDK_WALLET_LOCAL_CHAIN_PRIMARY_NAMESPACE,
		BDK_WALLET_LOCAL_CHAIN_SECONDARY_NAMESPACE, BDK_WALLET_TX_GRAPH_KEY,
		BDK_WALLET_TX_GRAPH_PRIMARY_NAMESPACE, BDK_WALLET_TX_GRAPH_SECONDARY_NAMESPACE,
	};
	use crate::logger::Logger;
	use crate::types::{DynStore, DynStoreWrapper};

	const EXTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
	const INTERNAL_DESCRIPTOR: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";

	fn hash(height: u32) -> BlockHash {
		let mut bytes = [0u8; 32];
		bytes[..4].copy_from_slice(&height.to_le_bytes());
		bytes[31] = 1;
		BlockHash::from_byte_array(bytes)
	}

	fn anchor(height: u32) -> ConfirmationBlockTime {
		ConfirmationBlockTime {
			block_id: BlockId { height, hash: hash(height) },
			confirmation_time: u64::from(height),
		}
	}

	fn change_set_with_chain(tip: u32, anchor_heights: &[u32]) -> ChangeSet {
		let mut change_set = ChangeSet::default();
		change_set.local_chain.blocks = (0..=tip).map(|h| (h, Some(hash(h)))).collect();
		change_set.tx_graph.anchors = anchor_heights
			.iter()
			.map(|h| (anchor(*h), Txid::from_byte_array([*h as u8; 32])))
			.collect();
		change_set
	}

	#[test]
	fn keeps_genesis_anchors_and_recent_blocks() {
		let tip = 1_000;
		let mut change_set = change_set_with_chain(tip, &[10, 500]);
		change_set.local_chain.blocks.insert(tip + 5, None);

		let removed = compact_local_chain(&mut change_set);

		let kept: Vec<u32> = change_set.local_chain.blocks.keys().copied().collect();
		let mut expected = vec![0, 10, 500];
		expected.extend(tip - LOCAL_CHAIN_RECENT_BLOCKS..=tip);
		assert_eq!(kept, expected);
		assert_eq!(removed, tip as usize + 2 - expected.len());
		assert!(change_set.local_chain.blocks.values().all(Option::is_some));

		let chain = LocalChain::from_changeset(change_set.local_chain.clone()).unwrap();
		assert_eq!(chain.tip().block_id(), BlockId { height: tip, hash: hash(tip) });
		assert_eq!(chain.get(500).map(|cp| cp.hash()), Some(hash(500)));

		// Compacting again is a no-op.
		assert_eq!(compact_local_chain(&mut change_set), 0);
	}

	#[test]
	fn short_chains_are_left_alone() {
		let mut change_set = change_set_with_chain(LOCAL_CHAIN_RECENT_BLOCKS, &[]);
		assert_eq!(compact_local_chain(&mut change_set), 0);
		assert_eq!(compact_local_chain(&mut ChangeSet::default()), 0);
	}

	#[tokio::test]
	async fn initialize_compacts_and_persists_the_local_chain() {
		let store: Arc<DynStore> = Arc::new(DynStoreWrapper(InMemoryStore::new()));
		let logger = Arc::new(Logger::new_log_facade());

		// Persist a wallet whose local chain has a checkpoint for every block.
		let mut persister = KVStoreWalletPersister::new(Arc::clone(&store), Arc::clone(&logger));
		AsyncWalletPersister::initialize(&mut persister).await.unwrap();
		let mut wallet = BdkWallet::create(EXTERNAL_DESCRIPTOR, INTERNAL_DESCRIPTOR)
			.network(Network::Regtest)
			.create_wallet_no_persist()
			.unwrap();
		let mut change_set = wallet.take_staged().unwrap();
		let genesis = change_set.local_chain.blocks[&0].unwrap();
		let mut long_chain = change_set_with_chain(2_000, &[42]);
		long_chain.local_chain.blocks.insert(0, Some(genesis));
		change_set.merge(long_chain);
		persister.persist_changeset(change_set).await.unwrap();

		let mut reloaded = KVStoreWalletPersister::new(Arc::clone(&store), Arc::clone(&logger));
		let loaded = AsyncWalletPersister::initialize(&mut reloaded).await.unwrap();
		assert_eq!(loaded.local_chain.blocks.len(), 2 + LOCAL_CHAIN_RECENT_BLOCKS as usize + 1);
		assert_eq!(loaded.local_chain.blocks[&0], Some(genesis));
		assert_eq!(loaded.local_chain.blocks[&42], Some(hash(42)));

		// The compacted chain was written back, and the wallet loads from it.
		let stored = read_bdk_wallet_local_chain(&*store, &logger).await.unwrap().unwrap();
		assert_eq!(stored, loaded.local_chain);
		let wallet = BdkWallet::load()
			.descriptor(bdk_wallet::KeychainKind::External, Some(EXTERNAL_DESCRIPTOR))
			.descriptor(bdk_wallet::KeychainKind::Internal, Some(INTERNAL_DESCRIPTOR))
			.extract_keys()
			.load_wallet_no_persist(loaded)
			.unwrap()
			.unwrap();
		assert_eq!(wallet.latest_checkpoint().height(), 2_000);
	}

	/// Replays a real wallet's persisted local chain and transaction graph, before and after
	/// compaction, through the two lookups that dominated a production node's CPU profile:
	/// BDK canonicalizing the graph, and the Esplora chain update looking up every anchor.
	///
	/// Run with `LDK_NODE_BENCH_LOCAL_CHAIN=<file> LDK_NODE_BENCH_TX_GRAPH=<file> cargo test
	/// --release -- --ignored --nocapture local_chain_compaction_benchmark`, the files holding
	/// the raw values of the `bdk_wallet/local_chain` and `bdk_wallet/tx_graph` keys.
	#[tokio::test]
	#[ignore]
	async fn local_chain_compaction_benchmark() {
		let local_chain_path = std::env::var("LDK_NODE_BENCH_LOCAL_CHAIN").unwrap();
		let tx_graph_path = std::env::var("LDK_NODE_BENCH_TX_GRAPH").unwrap();
		let store: Arc<DynStore> = Arc::new(DynStoreWrapper(InMemoryStore::new()));
		let logger = Arc::new(Logger::new_log_facade());
		KVStore::write(
			&*store,
			BDK_WALLET_LOCAL_CHAIN_PRIMARY_NAMESPACE,
			BDK_WALLET_LOCAL_CHAIN_SECONDARY_NAMESPACE,
			BDK_WALLET_LOCAL_CHAIN_KEY,
			std::fs::read(local_chain_path).unwrap(),
		)
		.await
		.unwrap();
		KVStore::write(
			&*store,
			BDK_WALLET_TX_GRAPH_PRIMARY_NAMESPACE,
			BDK_WALLET_TX_GRAPH_SECONDARY_NAMESPACE,
			BDK_WALLET_TX_GRAPH_KEY,
			std::fs::read(tx_graph_path).unwrap(),
		)
		.await
		.unwrap();
		let mut change_set = ChangeSet::default();
		change_set.local_chain =
			read_bdk_wallet_local_chain(&*store, &logger).await.unwrap().unwrap();
		change_set.tx_graph = read_bdk_wallet_tx_graph(&*store, &logger).await.unwrap().unwrap();
		let anchors = change_set.tx_graph.anchors.len();
		let mut graph = TxGraph::<ConfirmationBlockTime>::default();
		graph.apply_changeset(change_set.tx_graph.clone());
		let anchor_set = change_set.tx_graph.anchors.clone();

		let measure = |label: &str, blocks: &BTreeMap<u32, Option<BlockHash>>| {
			let chain = LocalChain::from_changeset(bdk_chain::local_chain::ChangeSet {
				blocks: blocks.clone(),
			})
			.unwrap();
			let tip = chain.tip();

			let start = Instant::now();
			let canonical = graph
				.list_canonical_txs(&chain, tip.block_id(), CanonicalizationParams::default())
				.filter(|tx| tx.chain_position.is_confirmed())
				.count();
			let canonicalize = start.elapsed();

			let start = Instant::now();
			let mut found = 0;
			for (anchor, _) in &anchor_set {
				if tip.get(anchor.block_id.height).is_some() {
					found += 1;
				}
			}
			let anchor_lookups = start.elapsed();
			println!(
				"{label}: {} checkpoints, {} txs confirmed, canonicalize {:?}, {} anchor lookups ({} found) {:?}",
				blocks.len(),
				canonical,
				canonicalize,
				anchors,
				found,
				anchor_lookups,
			);
			(canonical, found)
		};

		let before = measure("before", &change_set.local_chain.blocks);
		let start = Instant::now();
		let removed = compact_local_chain(&mut change_set);
		println!("compaction dropped {} checkpoints in {:?}", removed, start.elapsed());
		let after = measure("after", &change_set.local_chain.blocks);
		assert_eq!(before, after, "compaction must not change what the wallet sees");
	}

	/// Replays one incremental Esplora sync of a real wallet, watch-only from its persisted
	/// change set, as the node runs it: build the request, fetch, apply. Once with the local
	/// chain as persisted, once compacted with the update thinned.
	///
	/// Run with `LDK_NODE_BENCH_WALLET_DIR=<dir> LDK_NODE_BENCH_ESPLORA_URL=<url> cargo test
	/// --release -- --ignored --nocapture esplora_sync_benchmark`, the directory holding the raw
	/// values of the `bdk_wallet/*` keys as `<key>.bin`.
	#[cfg(feature = "chain-esplora")]
	#[tokio::test(flavor = "multi_thread")]
	#[ignore]
	async fn esplora_sync_benchmark() {
		use bdk_esplora::EsploraAsyncExt;

		let dir = std::path::PathBuf::from(std::env::var("LDK_NODE_BENCH_WALLET_DIR").unwrap());
		let esplora_url = std::env::var("LDK_NODE_BENCH_ESPLORA_URL").unwrap();
		let store: Arc<DynStore> = Arc::new(DynStoreWrapper(InMemoryStore::new()));
		let logger = Arc::new(Logger::new_log_facade());
		for key in
			["descriptor", "change_descriptor", "network", "local_chain", "tx_graph", "indexer"]
		{
			let value = std::fs::read(dir.join(format!("{key}.bin"))).unwrap();
			KVStore::write(&*store, "bdk_wallet", "", key, value).await.unwrap();
		}
		let persisted =
			crate::io::utils::read_bdk_wallet_change_set(&*store, &logger).await.unwrap().unwrap();
		let client = esplora_client::Builder::new(&esplora_url).build_async().unwrap();
		let concurrency = std::env::var("LDK_NODE_BENCH_CONCURRENCY")
			.map(|c| c.parse().unwrap())
			.unwrap_or(crate::config::BDK_CLIENT_CONCURRENCY);
		let compact_only = std::env::var("LDK_NODE_BENCH_COMPACT_ONLY").is_ok();

		for compact in [false, true] {
			if compact_only && !compact {
				continue;
			}
			let mut change_set = persisted.clone();
			if compact {
				compact_local_chain(&mut change_set);
			}
			let checkpoints = change_set.local_chain.blocks.len();
			let start = Instant::now();
			let mut wallet = BdkWallet::load().load_wallet_no_persist(change_set).unwrap().unwrap();
			let load = start.elapsed();

			let start = Instant::now();
			let request = wallet.start_sync_with_revealed_spks().build();
			let request_time = start.elapsed();
			let spks = wallet.spk_index().revealed_spks(..).count();

			let start = Instant::now();
			let mut update: bdk_wallet::Update =
				client.sync(request, concurrency).await.unwrap().into();
			let fetch = start.elapsed();

			let start = Instant::now();
			if compact {
				let local_tip = wallet.latest_checkpoint().height();
				if let Some(tip) = update.chain.take() {
					update.chain =
						Some(crate::wallet::thin_chain_update(tip, local_tip, &update.tx_update));
				}
			}
			wallet.apply_update_events(update).unwrap();
			let apply = start.elapsed();

			let start = Instant::now();
			let balance = wallet.balance();
			let balance_time = start.elapsed();
			println!(
				"{}: {} checkpoints, {} revealed spks, concurrency {}, load {:?}, request {:?}, fetch+chain update {:?}, apply {:?}, balance {:?} (confirmed {} sat), tip {}",
				if compact { "compacted" } else { "as persisted" },
				checkpoints,
				spks,
				concurrency,
				load,
				request_time,
				fetch,
				apply,
				balance_time,
				balance.confirmed.to_sat(),
				wallet.latest_checkpoint().height(),
			);
		}
	}
}
