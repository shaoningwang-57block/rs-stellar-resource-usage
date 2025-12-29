use soroban_client::account::Account;
use soroban_client::error::Error;
use soroban_client::soroban_rpc::GetEventsResponse;
use soroban_client::soroban_rpc::GetFeeStatsResponse;
use soroban_client::soroban_rpc::GetHealthResponse;
use soroban_client::soroban_rpc::GetLatestLedgerResponse;
use soroban_client::soroban_rpc::GetLedgerEntriesResponse;
use soroban_client::soroban_rpc::GetLedgersResponse;
use soroban_client::soroban_rpc::GetNetworkResponse;
use soroban_client::soroban_rpc::GetTransactionResponse;
use soroban_client::soroban_rpc::GetTransactionsResponse;
use soroban_client::soroban_rpc::GetVersionInfoResponse;
use soroban_client::soroban_rpc::LedgerEntryResult;
use soroban_client::soroban_rpc::SendTransactionResponse;
use soroban_client::soroban_rpc::SimulateTransactionResponse;
use soroban_client::soroban_rpc::TransactionStatus;
use soroban_client::transaction;
use soroban_client::transaction::Transaction;
use soroban_client::xdr::LedgerKey;
use soroban_client::xdr::ScVal;
use soroban_client::Durability;
use soroban_client::EventFilter;
use soroban_client::Options;
use soroban_client::Pagination;
use soroban_client::Server;
use soroban_client::SimulationOptions;
use std::collections::HashMap;
use std::time::Duration;

use crate::show;
use crate::statistics;
use crate::statistics::ResourceMetric;

const WAIT_TIME: u64 = 10;

#[derive(Debug, Clone)]
pub struct HashMapValue {
    // send_tx_res: StellarTransactionResp,
    transaction: Transaction,
    sim_tx_res: SimulateTransactionResponse,
}

pub type FunctionStore = HashMap<String, Vec<ResourceMetric>>;
pub type ContractStore = HashMap<String, FunctionStore>;

#[derive(Debug)]
pub struct StellarRpcServer {
    inner: soroban_client::Server,
    hash: HashMap<String, HashMapValue>,
    transaction: Option<Transaction>,
    sim_tx_res: Option<SimulateTransactionResponse>,
    store_stats: ContractStore,
}

impl StellarRpcServer {
    pub fn new(url: &str, opts: Options) -> Result<Self, Error> {
        Ok(Self {
            inner: Server::new(url, opts)?,
            hash: HashMap::new(),
            transaction: None,
            sim_tx_res: None,
            store_stats: HashMap::new(),
        })
    }
    //
    // override function
    //
    pub async fn simulate_transaction(
        &mut self,
        tx: &Transaction,
        leeway: Option<SimulationOptions>,
    ) -> Result<SimulateTransactionResponse, Error> {
        let sim = self.inner.simulate_transaction(tx, leeway).await?;
        self.transaction = Some(tx.clone());
        self.sim_tx_res = Some(sim.clone());
        Ok(sim)
    }

    pub async fn prepare_transaction(
        &mut self,
        transaction: &Transaction,
    ) -> Result<Transaction, Error> {
        let sim_response = self.simulate_transaction(transaction, None).await?;
        transaction::assemble_transaction(transaction, sim_response)
    }

    pub async fn send_transaction(
        &mut self,
        tx: Transaction,
    ) -> Result<SendTransactionResponse, Error> {
        // println!("send_trasaction: sim_tx_res:{:?}", self.sim_tx_res);
        // println!("send_trasaction: trasaction:{:?}", self.transaction);
        let res = self.inner.send_transaction(tx.clone()).await?;
        if let (Some(sim), Some(prev_tx)) = (&self.sim_tx_res, &self.transaction) {
            self.hash.insert(
                res.hash.clone(),
                HashMapValue {
                    transaction: prev_tx.clone(),
                    sim_tx_res: sim.clone(),
                },
            );
        }
        self.transaction = None;
        self.sim_tx_res = None;
        Ok(res)
    }

    pub async fn print_table(&mut self) -> Result<(), crate::Error> {
        let hashes: Vec<String> = self.hash.keys().cloned().collect();
        let server = &self.inner;
        let futures = hashes.iter().map(|h| {
            let h = h.clone();
            async move {
                let res = server
                    .wait_transaction(&h, Duration::from_secs(WAIT_TIME))
                    .await;
                (h, res)
            }
        });
        let results = futures::future::join_all(futures).await;
        for (hash, tx_result) in results {
            let Ok(tx_result) = tx_result else {
                println!("fail to get transaction");
                continue;
            };
            if tx_result.status != TransactionStatus::Success {
                println!("transaction status error: {:?}", tx_result.status);
                continue;
            };
            let Some(map_value) = self.hash.get(&hash) else {
                continue;
            };
            let stats = statistics::handle_transaction(&map_value.sim_tx_res, &tx_result)?;
            statistics::store_transaction(&mut self.store_stats, &map_value.transaction, &stats);
        }
        for (constract_id, _) in &self.store_stats {
            show::print_table(constract_id, &self.store_stats)
        }
        self.hash.clear();
        self.store_stats.clear();
        Ok(())
    }

    //
    // inner function
    //
    pub async fn get_events(
        &self,
        ledger: Pagination,
        filters: Vec<EventFilter>,
        limit: impl Into<Option<u32>>,
    ) -> Result<GetEventsResponse, Error> {
        self.inner.get_events(ledger, filters, limit).await
    }
    pub async fn get_fee_stats(&self) -> Result<GetFeeStatsResponse, Error> {
        self.inner.get_fee_stats().await
    }

    pub async fn get_health(&self) -> Result<GetHealthResponse, Error> {
        self.inner.get_health().await
    }

    pub async fn get_latest_ledger(&self) -> Result<GetLatestLedgerResponse, Error> {
        self.inner.get_latest_ledger().await
    }

    pub async fn get_ledger_entries(
        &self,
        keys: Vec<LedgerKey>,
    ) -> Result<GetLedgerEntriesResponse, Error> {
        self.inner.get_ledger_entries(keys).await
    }

    pub async fn get_ledgers(
        &self,
        ledger: Pagination,
        limit: impl Into<Option<u32>>,
    ) -> Result<GetLedgersResponse, Error> {
        self.inner.get_ledgers(ledger, limit).await
    }

    pub async fn get_network(&self) -> Result<GetNetworkResponse, Error> {
        self.inner.get_network().await
    }

    pub async fn get_transaction(&self, hash: &str) -> Result<GetTransactionResponse, Error> {
        self.inner.get_transaction(hash).await
    }

    pub async fn get_transactions(
        &self,
        ledger: Pagination,
        limit: impl Into<Option<u32>>,
    ) -> Result<GetTransactionsResponse, Error> {
        self.inner.get_transactions(ledger, limit).await
    }

    pub async fn get_version_info(&self) -> Result<GetVersionInfoResponse, Error> {
        self.inner.get_version_info().await
    }

    pub async fn get_account(&self, address: &str) -> Result<Account, Error> {
        self.inner.get_account(address).await
    }

    pub async fn get_contract_data(
        &self,
        contract: &str,
        key: ScVal,
        durability: Durability,
    ) -> Result<LedgerEntryResult, Error> {
        self.inner
            .get_contract_data(contract, key, durability)
            .await
    }

    pub async fn request_airdrop(&self, account_id: &str) -> Result<Account, Error> {
        self.inner.request_airdrop(account_id).await
    }

    pub async fn wait_transaction(
        &self,
        hash: &str,
        max_wait: Duration,
    ) -> Result<GetTransactionResponse, (Error, Option<GetTransactionResponse>)> {
        self.inner.wait_transaction(hash, max_wait).await
    }
}
