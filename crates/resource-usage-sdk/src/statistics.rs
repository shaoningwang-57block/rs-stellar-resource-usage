use crate::Error;
use std::collections::HashMap;

use crate::{rpc_server::ContractStore, scval_tools};
use soroban_client::{
    soroban_rpc::{GetTransactionResponse, SimulateTransactionResponse},
    transaction::Transaction,
    xdr::{
        ContractEventBody, HostFunction, LedgerEntryChange, Limits, OperationBody, ScAddress,
        TransactionMeta, TransactionMetaV4, WriteXdr,
    },
};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ResourceMetric {
    pub cpu_insns: Option<u64>,
    pub mem_bytes: Option<u64>,
    pub entry_bytes: Option<usize>,
    pub entry_reads: Option<usize>,
    pub entry_writes: Option<usize>,
    pub read_bytes: Option<u32>,
    pub write_bytes: Option<u32>,
    pub min_txn_bytes: Option<usize>,
}

// xdr safe parameter
const LIMITS: Limits = Limits {
    depth: 200,           // 暂定200
    len: 2 * 1024 * 1024, // 2MB
};

// statistic simulate and transaction usage
pub fn handle_transaction(
    sim_tx: &SimulateTransactionResponse,
    tx_result: &GetTransactionResponse,
) -> Result<ResourceMetric, Error> {
    let (meta, _) = tx_result.to_result_meta().ok_or(Error::MissingMeta)?;
    match meta {
        // TransactionMeta::V1(m) => handle_meta_v1(sim_tx, tx_result, &m),
        // TransactionMeta::V2(m) => handle_meta_v2(sim_tx, tx_result, &m),
        // TransactionMeta::V3(m) => handle_meta_v3(sim_tx, tx_result, &m),
        TransactionMeta::V4(m) => handle_meta_v4(sim_tx, tx_result, &m),
        _ => Err(Error::UnsupportedMeta),
    }
}

// meta v4 support
pub fn handle_meta_v4(
    sim_tx: &SimulateTransactionResponse,
    tx_result: &GetTransactionResponse,
    meta: &TransactionMetaV4,
) -> Result<ResourceMetric, Error> {
    let Some(sim_transaction) = sim_tx.to_transaction_data() else {
        return Err(Error::NoTransactionData);
    };
    let resource = sim_transaction.resources;
    let footprint = resource.footprint;
    let entry_reads = footprint.read_only.len();
    let entry_writes = footprint.read_write.len();
    let read_bytes = resource.disk_read_bytes;
    let write_bytes = resource.write_bytes;
    let min_txn_bytes = tx_result.to_envelope().to_xdr(LIMITS.clone())?.len();
    let entry_bytes = max_entry_value_len(&meta, LIMITS.clone());
    let metrics = get_core_metrics(&meta);
    return Ok(ResourceMetric {
        cpu_insns: metrics.cpu_insn,
        mem_bytes: metrics.mem_byte,
        entry_bytes: Some(entry_bytes),
        entry_reads: Some(entry_reads),
        entry_writes: Some(entry_writes),
        read_bytes: Some(read_bytes),
        write_bytes: Some(write_bytes),
        min_txn_bytes: Some(min_txn_bytes),
    });
}

// find out max len in operation-change
fn max_entry_value_len(meta: &TransactionMetaV4, limits: Limits) -> usize {
    let mut max_len = 0usize;
    for op in meta.operations.iter() {
        for change in op.changes.iter() {
            let xdr_limit = limits.clone();
            let len = match change {
                LedgerEntryChange::Created(created) => {
                    created.data.to_xdr(xdr_limit).map(|b| b.len()).unwrap_or(0)
                }
                LedgerEntryChange::Updated(updated) => {
                    updated.data.to_xdr(xdr_limit).map(|b| b.len()).unwrap_or(0)
                }
                _ => 0,
            };
            if len > max_len {
                max_len = len;
            }
        }
    }
    max_len
}

#[derive(Default)]
struct Metrics {
    cpu_insn: Option<u64>,
    mem_byte: Option<u64>,
    // ledger_read_byte: Option<u64>,
    // ledger_write_byte: Option<u64>,
}

const CORE_KEYS: [&str; 4] = [
    "cpu_insn",
    "mem_byte",
    "ledger_read_byte",
    "ledger_write_byte",
];

// get core metrics from events
fn get_core_metrics(meta: &TransactionMetaV4) -> Metrics {
    let mut map: HashMap<&'static str, u64> = HashMap::new();
    for te in meta.diagnostic_events.iter() {
        let body = match &te.event.body {
            ContractEventBody::V0(v0) => v0,
        };
        let mut is_core_metrics = false;
        let mut matched_key: Option<&'static str> = None;
        for topic in body.topics.iter().filter_map(scval_tools::scval_as_string) {
            if topic == "core_metrics" {
                is_core_metrics = true;
                continue;
            }
            if matched_key.is_none() {
                if let Some(k) = CORE_KEYS.iter().copied().find(|k| *k == topic) {
                    matched_key = Some(k);
                }
            }
        }
        if !is_core_metrics {
            continue;
        }
        let Some(key) = matched_key else {
            continue;
        };
        let Some(v) = scval_tools::scval_as_u64(&body.data) else {
            continue;
        };
        map.insert(key, v);
    }
    Metrics {
        cpu_insn: map.get("cpu_insn").copied(),
        mem_byte: map.get("mem_byte").copied(),
        // ledger_read_byte: map.get("ledger_read_byte").copied(),
        // ledger_write_byte: map.get("ledger_write_byte").copied(),
    }
}

// store transation usage stats
pub fn store_transaction(
    store_stats: &mut ContractStore,
    transaction: &Transaction,
    stats: &ResourceMetric,
) {
    let Some(operations) = &transaction.operations else {
        return;
    };
    for operation in operations.iter() {
        let invoke_op = match &operation.body {
            OperationBody::InvokeHostFunction(invoke_op) => invoke_op,
            _ => continue,
        };
        let args = match &invoke_op.host_function {
            HostFunction::InvokeContract(invoke_contract) => invoke_contract,
            _ => continue,
        };
        let contract_id = match &args.contract_address {
            ScAddress::Contract(contract) => contract,
            _ => continue,
        };
        let str_key = stellar_strkey::Contract(contract_id.as_ref().0);
        let function_name = &args.function_name.0.to_string();
        // Rust: stored_stats[contract_id][func_name].push(stats)
        store_stats
            .entry(str_key.to_string())
            .or_default()
            .entry(function_name.clone())
            .or_default()
            .push(stats.clone());
    }
}
