use soroban_client::error::Error;
use soroban_client::{
    account::{Account, AccountBehavior},
    address::{Address, AddressTrait},
    contract::{ContractBehavior, Contracts},
    keypair::{Keypair, KeypairBehavior},
    network::{NetworkPassphrase, Networks},
    operation::{self, Operation},
    soroban_rpc::TransactionStatus,
    transaction::{TransactionBehavior, TransactionBuilder, TransactionBuilderBehavior},
    xdr, Options, Server,
};
use std::time::Duration;

struct Client {
    source_keypair: Keypair,
    source_account: Account,
    server: Server,
}

impl Client {
    async fn new(url: &str) -> Result<Client, Error> {
        let mut options = Options::default();
        options.allow_http = true;
        let server = Server::new(url, options).expect("Cannot create server");
        let source_keypair = Keypair::random().unwrap();
        let source_public_key = &source_keypair.public_key();
        // Get account information from server
        let account_data = server.request_airdrop(source_public_key).await?;
        let source_account =
            Account::new(source_public_key, &account_data.sequence_number()).unwrap();
        return Ok(Client {
            source_keypair,
            source_account,
            server,
        });
    }

    async fn deploy_contact(&mut self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let wasm;
        match std::fs::read(path) {
            Ok(read_data) => {
                wasm = read_data;
            }
            Err(_error) => return Err("file path error".into()),
        };
        //
        // Uploading the WASM executable
        //
        let upload = Operation::new()
            .upload_wasm(&wasm, None)
            .expect("Cannot create upload_wasm operation");
        let tx = TransactionBuilder::new(&mut self.source_account, Networks::standalone(), None)
            .fee(1000u32)
            .add_operation(upload)
            .build();
        let signers = [self.source_keypair.clone()];
        let mut ptx = self.server.prepare_transaction(&tx).await?;
        ptx.sign(&signers);

        println!("> Uploading WASM executable");
        let response = self.server.send_transaction(ptx).await?;

        let hash = response.hash;
        println!(">> Tx hash: {hash}");
        let wasm_hash = match self
            .server
            .wait_transaction(&hash, Duration::from_secs(15))
            .await
        {
            Ok(tx_result) if tx_result.status == TransactionStatus::Success => {
                let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta found");
                if let Some(scval) = ret_val {
                    let bytes: Vec<u8> = scval.try_into().expect("Cannot convert ScVal to Vec<u8>");
                    *bytes.last_chunk::<32>().expect("Not 32 bytes")
                } else {
                    return Err(">> None return value".into());
                }
            }
            _ => {
                println!(">> Failed to upload the WASM executable");
                return Err(">> Failed to upload the wasm".into());
            }
        };
        println!(">> Wasm hash: {}", hex::encode(wasm_hash));
        println!();

        //
        // Create the contract for the uploaded WASM
        //
        let create_contract = Operation::new()
            .create_contract(
                &self.source_keypair.public_key(),
                wasm_hash,
                None,
                None,
                [].into(),
            )
            .expect("Cannot create create_contract operation");
        let tx = TransactionBuilder::new(&mut self.source_account, Networks::standalone(), None)
            .fee(1000u32)
            .add_operation(create_contract)
            .build();

        let mut ptx = self.server.prepare_transaction(&tx).await?;
        ptx.sign(&signers);

        println!(
            "> Creating the contract for WASM hash {}",
            hex::encode(wasm_hash)
        );
        let response = self.server.send_transaction(ptx).await?;

        let hash = response.hash;
        println!(">> Tx hash: {hash}");
        let contract_addr = match self
            .server
            .wait_transaction(&hash, Duration::from_secs(15))
            .await
        {
            Ok(tx_result) if tx_result.status == TransactionStatus::Success => {
                let (_meta, ret_val) = tx_result.to_result_meta().expect("No meta");
                if let Some(xdr::ScVal::Address(addr)) = ret_val {
                    Address::from_sc_address(&addr).unwrap()
                } else {
                    return Err("Failed to create contract".into());
                }
            }
            _ => return Err("Failed to create contract".into()),
        };
        println!(">> Contract id: {}", contract_addr.to_string());
        println!();
        Ok(())
    }

    async fn airdrop(&self, public_key: &String) -> Result<Account, Error> {
        self.server.request_airdrop(public_key).await
    }

    pub async fn create_account(
        &mut self,
        public_key: &String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get account information from server
        let create_account_op = Operation::new()
            .create_account(public_key, operation::ONE)
            .expect("Cannot create operation");

        let mut builder =
            TransactionBuilder::new(&mut self.source_account, Networks::standalone(), None);
        builder.fee(1000u32);
        builder.add_operation(create_account_op);

        let mut tx = builder.build();
        let source_keypair = self.source_keypair.clone();
        tx.sign(&[source_keypair]);
        let response = self.server.send_transaction(tx).await?;
        println!("response:{:?}", response);
        println!("response error result:{:?}", response.to_error_result());

        let hash = response.hash;

        println!(">> Tx hash: {hash}");
        match self
            .server
            .wait_transaction(&hash, Duration::from_secs(15))
            .await
        {
            Ok(tx_result) => match tx_result.status {
                TransactionStatus::Success => {
                    println!("Transaction successful!");
                    println!();
                    if let Some(ledger) = tx_result.ledger {
                        println!("Confirmed in ledger: {}", ledger);
                    }
                    Ok(())
                }
                TransactionStatus::Failed => {
                    if let Some(result) = tx_result.to_result() {
                        eprintln!("Transaction failed with result: {:?}", result);
                    } else {
                        eprintln!("Transaction failed without result XDR");
                    }
                    Ok(())
                }
                TransactionStatus::NotFound => {
                    eprintln!("Transaction not found");
                    Err("Transaction not found".into())
                }
            },
            Err(e) => Err(e.0.to_string().into()),
        }
    }

    async fn invoke(
        &mut self,
        contract_addr: Address,
        function: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let signers = [self.source_keypair.clone()];
        let contract = Contracts::new(&contract_addr.to_string()).unwrap();

        let tx = TransactionBuilder::new(&mut self.source_account, Networks::standalone(), None)
            .fee(1000u32)
            .add_operation(contract.call(
                "increment",
                Some(vec![
                    Address::account(signers[0].raw_public_key())?.to_sc_val()?,
                    3u32.into(),
                ]),
            ))
            .build();

        let mut ptx = self.server.prepare_transaction(&tx).await?;
        ptx.sign(&signers);

        println!(
            "> Calling increment on contract {}",
            contract_addr.to_string()
        );
        let response = self.server.send_transaction(ptx).await?;

        let hash = response.hash;
        println!(">> Tx hash: {hash}");
        match self
            .server
            .wait_transaction(&hash, Duration::from_secs(15))
            .await
        {
            Ok(tx_result) if tx_result.status == TransactionStatus::Success => {
                let (_meta, ret_val) = tx_result.to_result_meta().expect("No result meta");
                ret_val
                    .expect("None returned value")
                    .try_into()
                    .expect("Return value is not u32")
            }
            _ => return Err("Failed to create contract".into()),
        };
        Ok(())
    }
}

#[tokio::test]
async fn test() {
    let mut client = Client::new("http://localhost:8000/rpc").await.unwrap();
    let keypair = Keypair::random().unwrap();
    client.create_account(&keypair.public_key()).await.unwrap();
    // client.airdrop(&"".to_string());
    // client.deploy_contact("");
    // let address = Address::new("").unwrap();
    // client.invoke(address, "increment", []);
}
