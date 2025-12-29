use crate::rpc_server::ContractStore;
use crate::statistics::ResourceMetric;
use std::collections::HashMap;

use comfy_table::{
    presets::UTF8_FULL, Attribute, Cell, CellAlignment, Color, ContentArrangement, Table,
};

#[derive(Clone, Copy, Debug)]
pub struct LimitsCursors {
    pub danger: f64, // 0.8 => 80%
    pub error: f64,  // 1.0 => 100%
}

#[derive(Clone, Debug)]
pub struct MetricStatistics {
    pub avg: f64,
    pub max: u64,
    pub min: u64,
    pub sum: u128,
}

pub type ResultStatistics = HashMap<String, HashMap<String, FuncStatistics>>;

#[derive(Clone, Debug)]
pub struct FuncStatistics {
    pub times: usize,
    pub metrics: HashMap<&'static str, MetricStatistics>,
}

#[derive(Clone, Debug)]
pub struct FuncTableData {
    pub func: String,
    pub times: usize,
    pub rows: Vec<(&'static str, u64, f64, u64, u64, u128)>, // (key, limit, avg, max, min, sum)
}

const METRIC_KEYS: [&'static str; 8] = [
    "cpu_insns",
    "mem_bytes",
    "entry_bytes",
    "entry_reads",
    "entry_writes",
    "read_bytes",
    "write_bytes",
    "min_txn_bytes",
];

const METRIC_KEYS_FOR_PRINT: [&'static str; 8] = METRIC_KEYS;

fn stellar_limits_config() -> HashMap<&'static str, u64> {
    HashMap::from([
        ("cpu_insns", 50_000_000),
        ("mem_bytes", 10_000_000),
        ("entry_bytes", 1_000_000),
        ("entry_reads", 10_000),
        ("entry_writes", 10_000),
        ("read_bytes", 2_000_000),
        ("write_bytes", 2_000_000),
        ("min_txn_bytes", 100_000),
    ])
}

fn get_metric_u64(m: &ResourceMetric, key: &str) -> Option<u64> {
    match key {
        "cpu_insns" => m.cpu_insns,
        "mem_bytes" => m.mem_bytes,
        "entry_bytes" => m.entry_bytes.map(|v| v as u64),
        "entry_reads" => m.entry_reads.map(|v| v as u64),
        "entry_writes" => m.entry_writes.map(|v| v as u64),
        "read_bytes" => m.read_bytes.map(|v| v as u64),
        "write_bytes" => m.write_bytes.map(|v| v as u64),
        "min_txn_bytes" => m.min_txn_bytes.map(|v| v as u64),
        _ => None,
    }
}

pub fn calc_statistics(store: &ContractStore) -> ResultStatistics {
    let mut res: ResultStatistics = HashMap::new();

    for (contract_name, funcs) in store {
        let contract_entry = res
            .entry(contract_name.clone())
            .or_insert_with(HashMap::new);

        for (func_name, data) in funcs {
            let times = data.len();
            if times == 0 {
                continue;
            }

            let mut func_stats = FuncStatistics {
                times,
                metrics: HashMap::new(),
            };

            for key in METRIC_KEYS.iter() {
                // TS: if (!data[0][key]) return;
                let first_val = match get_metric_u64(&data[0], key) {
                    Some(v) => v,
                    None => continue,
                };

                let mut sum: u128 = 0;
                let mut max: u64 = first_val;
                let mut min: u64 = first_val;

                for metric in data.iter() {
                    let value = get_metric_u64(metric, key).unwrap_or(0);
                    sum += value as u128;
                    if value > max {
                        max = value;
                    }
                    if value < min {
                        min = value;
                    }
                }

                let avg = sum as f64 / times as f64;

                func_stats
                    .metrics
                    .insert(*key, MetricStatistics { avg, max, min, sum });
            }

            contract_entry.insert(func_name.clone(), func_stats);
        }
    }

    res
}

pub fn load_table_data(
    statistics: &ResultStatistics,
    limits: &HashMap<&'static str, u64>,
) -> Vec<FuncTableData> {
    let mut res: Vec<FuncTableData> = vec![];

    for (_contract, funcs) in statistics {
        for (func, data) in funcs {
            let mut rows: Vec<(&'static str, u64, f64, u64, u64, u128)> = vec![];

            for key in METRIC_KEYS_FOR_PRINT.iter() {
                let Some(stat) = data.metrics.get(key) else {
                    continue;
                };
                let Some(limit) = limits.get(key) else {
                    continue;
                };
                if *limit == 0 {
                    continue;
                }

                rows.push((*key, *limit, stat.avg, stat.max, stat.min, stat.sum));
            }

            res.push(FuncTableData {
                func: func.clone(),
                times: data.times,
                rows,
            });
        }
    }

    res
}

fn cyan_bold<S: Into<String>>(s: S) -> Cell {
    Cell::new(s.into())
        .fg(Color::Cyan)
        .add_attribute(Attribute::Bold)
}

fn yellow_bold<S: Into<String>>(s: S) -> Cell {
    Cell::new(s.into())
        .fg(Color::Yellow)
        .add_attribute(Attribute::Bold)
}

fn red_bold<S: Into<String>>(s: S) -> Cell {
    Cell::new(s.into())
        .fg(Color::Red)
        .add_attribute(Attribute::Bold)
}

fn center(cell: Cell) -> Cell {
    cell.set_alignment(CellAlignment::Center)
}

fn format_cell_f64(value: f64, limit: u64, cursors: LimitsCursors) -> Cell {
    let percent = (value / limit as f64) * 100.0;
    let is_danger = percent > cursors.danger * 100.0;
    let is_error = percent > cursors.error * 100.0;

    let mut cell = Cell::new(format!("{value:.2}"));
    if is_error {
        cell = cell.fg(Color::Red).add_attribute(Attribute::Bold);
    } else if is_danger {
        cell = cell.fg(Color::Yellow).add_attribute(Attribute::Bold);
    }
    cell
}

fn format_cell_u64(value: u64, limit: u64, cursors: LimitsCursors) -> Cell {
    let percent = (value as f64 / limit as f64) * 100.0;
    let is_danger = percent > cursors.danger * 100.0;
    let is_error = percent > cursors.error * 100.0;

    let mut cell = Cell::new(value.to_string());
    if is_error {
        cell = cell.fg(Color::Red).add_attribute(Attribute::Bold);
    } else if is_danger {
        cell = cell.fg(Color::Yellow).add_attribute(Attribute::Bold);
    }
    cell
}

pub fn print_table(contract_id: &str, store: &ContractStore) {
    let cursors = LimitsCursors {
        danger: 0.8,
        error: 1.0,
    };

    let limits = stellar_limits_config();
    let statistics = calc_statistics(store);
    let mut funcs = load_table_data(&statistics, &limits);

    funcs.sort_by(|a, b| a.func.cmp(&b.func));

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_content_arrangement(ContentArrangement::Dynamic);

    table.add_row(vec![
        Cell::new(""),
        Cell::new(""),
        center(cyan_bold("Resource Usage Table")),
        Cell::new(""),
        Cell::new(""),
        Cell::new(""),
    ]);

    table.add_row(vec![
        cyan_bold("Highligh Color"),
        Cell::new(""),
        center(yellow_bold(format!(
            "Warning: {}% - {}%",
            (cursors.danger * 100.0) as u64,
            (cursors.error * 100.0) as u64
        ))),
        Cell::new(""),
        center(red_bold(format!(
            "Error: Over {}%",
            (cursors.error * 100.0) as u64
        ))),
        Cell::new(""),
    ]);

    table.add_row(vec![
        cyan_bold("Contract"),
        Cell::new(""),
        Cell::new(contract_id),
        Cell::new(""),
        Cell::new(""),
        Cell::new(""),
    ]);

    for f in funcs {
        table.add_row(vec![
            cyan_bold("Function"),
            Cell::new(f.func),
            Cell::new(""),
            cyan_bold("Times"),
            Cell::new(f.times.to_string()),
            Cell::new(""),
        ]);

        table.add_row(vec![
            cyan_bold("Resource"),
            cyan_bold("Limitation"),
            cyan_bold("Avg"),
            cyan_bold("Max"),
            cyan_bold("Min"),
            cyan_bold("Sum"),
        ]);

        for (key, limit, avg, max, min, sum) in f.rows {
            table.add_row(vec![
                cyan_bold(key),
                Cell::new(limit.to_string()),
                format_cell_f64(avg, limit, cursors),
                format_cell_u64(max, limit, cursors),
                format_cell_u64(min, limit, cursors),
                Cell::new(sum.to_string()),
            ]);
        }
    }

    println!("{table}");
}

// use crate::rpc_server::FunctionStore;
// #[test]
// fn test() {
//     let mut store: ContractStore = HashMap::new();
//     let mut funcs: FunctionStore = HashMap::new();

//     // swap: 5 次
//     let mut swap_samples: Vec<ResourceMetric> = vec![];
//     for i in 0..5 {
//         swap_samples.push(ResourceMetric {
//             cpu_insns: Some(10_000_000 + i * 12_000_000),
//             mem_bytes: Some(2_000_000 + i * 900_000),
//             read_bytes: Some((500_000 + i * 150_000) as u32),
//             write_bytes: Some((300_000 + i * 120_000) as u32),
//             min_txn_bytes: Some((90_000 + i * 8_000) as usize),
//             ..Default::default()
//         });
//     }
//     funcs.insert("swap".into(), swap_samples);

//     // add_liquidity: 3 次
//     let mut add_samples: Vec<ResourceMetric> = vec![];
//     for i in 0..3 {
//         add_samples.push(ResourceMetric {
//             cpu_insns: Some(25_000_000 + i * 10_000_000),
//             mem_bytes: Some(4_000_000 + i * 2_500_000),
//             entry_reads: Some((3000 + i * 6500) as usize),
//             entry_writes: Some((2200 + i * 7500) as usize),
//             ..Default::default()
//         });
//     }
//     funcs.insert("add_liquidity".into(), add_samples);

//     store.insert("contractA".into(), funcs);

//     print_table("CABC...1234", &store);
// }
