use soroban_client::{
    account::{Account, AccountBehavior},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    Options,
};

use resource_usage_sdk::StellarRpcServer;

#[tokio::main]
pub async fn main() {
    test_my_server().await;
}

async fn test_my_server() {
    let mut opts = Options::default();
    opts.allow_http = true;
    let mut server =
        StellarRpcServer::new("http://localhost:8000/rpc", opts).expect("start_server_error");
    let source_keypair = Keypair::random().unwrap();
    let source_public_key = &source_keypair.public_key();
    let signers = [source_keypair];

    // Get account information from server
    let account_data = server.request_airdrop(source_public_key).await.unwrap();
    let mut source_account =
        Account::new(source_public_key, &account_data.sequence_number()).unwrap();
    println!("source_account:{:?}", source_account);
    //
    // Calling the increment method of the contract
    //
    let contract_id = "CDGBYRHOQYXKIY37TI5BWNU2JYYC4KWNNU6MV2R7DWU45IVFXHQ5F6ZY";
    let contract = Contracts::new(contract_id).unwrap();
    let tx = TransactionBuilder::new(&mut source_account, Networks::standalone(), None)
        .fee(1000u32)
        .add_operation(contract.call(
            "increment",
            Some(vec![
                // Address::account(signers[0].raw_public_key()).unwrap().to_sc_val().unwrap(),
                // 3u32.into(),
            ]),
        ))
        .build();
    let mut ptx = server.prepare_transaction(&tx).await.unwrap();
    ptx.sign(&signers);
    println!("> Calling increment on contract {}", contract_id);
    println!("server:{:?}", server);
    let response = server.send_transaction(ptx).await.unwrap();
    let hash = response.hash;
    println!(">> Tx hash: {hash}");
    let result = server.print_table().await;
    println!("print_table_result:{:?}", result);
    println!();
}
