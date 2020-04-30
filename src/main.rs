mod evmone;

use std::collections::HashMap;
use std::fs;

use clap::{App, Arg};
use evmc_sys as ffi;
use evmone::{
    get_interface, Address, Bytes32, CallKind, Evmone, ExecutionContext, ExecutionMessage,
    ExecutionResult, HostContext, HostContextPtr, HostInterface, Revision, StorageStatus,
    TxContext, Uint256,
};
use serde::{Deserialize, Serialize};

const SIMPLE_STORAGE_CODE: &str = "60806040525b607b60006000508190909055505b610018565b60db806100266000396000f3fe60806040526004361060295760003560e01c806360fe47b114602f5780636d4ce63c14605b576029565b60006000fd5b60596004803603602081101560445760006000fd5b81019080803590602001909291905050506084565b005b34801560675760006000fd5b50606e6094565b6040518082815260200191505060405180910390f35b8060006000508190909055505b50565b6000600060005054905060a2565b9056fea26469706673582212204e58804e375d4a732a7b67cce8d8ffa904fa534d4555e655a433ce0a5e0d339f64736f6c63430006060033";

fn main() {
    let matches = App::new("Play evmone")
        .arg(
            Arg::with_name("code")
                .long("code")
                .short("c")
                .takes_value(true)
                .required(true)
                .help("The binary code path"),
        )
        .arg(
            Arg::with_name("input-data")
                .long("input-data")
                .short("i")
                .takes_value(true)
                .help("The input data for the contract"),
        )
        .arg(
            Arg::with_name("storage")
                .long("storage")
                .short("s")
                .takes_value(true)
                .help("The storage to run the contract"),
        )
        .get_matches();

    for i in 0..2 {
        let host_context = Box::new(TestHostContext::new(0));
        let host_context_ptr = HostContextPtr::from(host_context);
        let mut context = ExecutionContext::new(TestHostContext::interface(), host_context_ptr.ptr);
        let instance = Evmone::new();

        let destination = Address([32u8; 20]);
        let sender = Address([128u8; 20]);
        let value = Uint256([1u8; 32]);
        let create2_salt = Bytes32([255u8; 32]);

        let code = load_binary(matches.value_of("code").unwrap());
        let input_data = if let Some(input_data_path) = matches.value_of("input-data") {
            load_binary(&input_data_path)
        } else {
            Vec::new()
        };

        let raw_message = ffi::evmc_message {
            kind: CallKind::EVMC_CREATE,
            flags: 44,
            depth: 0,
            gas: 4_466_666,
            destination: destination.into(),
            sender: sender.into(),
            input_data: input_data.as_ptr(),
            input_size: input_data.len(),
            value: value.into(),
            create2_salt: create2_salt.into(),
        };
        let message = ExecutionMessage::from(&raw_message);

        let result = instance.execute(Revision::EVMC_PETERSBURG, &code, &message, &mut context);
        println!("[Round {}] Execution result: {:?}\n", i, result);
    }
}

fn load_binary(path: &str) -> Vec<u8> {
    hex::decode(
        String::from_utf8(fs::read(path).unwrap())
            .unwrap()
            .trim()
            .as_bytes(),
    )
    .unwrap()
}

#[derive(Debug, Clone, Eq, PartialEq, Default, Deserialize, Serialize)]
pub struct Value {
    data: Bytes32,
    // Modified time:
    //   0 => first set
    //   1 => modified
    //   2..n => modifled again
    modified: usize,
}

impl Value {
    fn new(data: Bytes32) -> Value {
        Value { data, modified: 0 }
    }

    fn update_data(&mut self, data: Bytes32) -> bool {
        if data != self.data {
            self.data = data;
            self.modified += 1;
            true
        } else {
            false
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct TestHostContext {
    pub depth: u32,
    pub code: Vec<u8>,
    pub storage: HashMap<Address, HashMap<Bytes32, Value>>,
}

impl TestHostContext {
    pub fn new(depth: u32) -> TestHostContext {
        TestHostContext {
            depth,
            code: Vec::new(),
            storage: HashMap::default(),
        }
    }
}

impl HostContext for TestHostContext {
    fn interface() -> HostInterface {
        get_interface::<TestHostContext>()
    }

    fn get_tx_context(&mut self) -> TxContext {
        TxContext {
            tx_gas_price: Uint256::default().into(),
            tx_origin: Address([10u8; 20]).into(),
            block_coinbase: Address::default().into(),
            block_number: 0,
            block_timestamp: 0,
            block_gas_limit: 666_666_666,
            block_difficulty: Uint256::default().into(),
            chain_id: Uint256::default().into(),
        }
    }

    fn account_exists(&mut self, address: &Address) -> bool {
        println!("account_exists(address: {:?})", address);
        true
    }

    fn get_storage(&mut self, address: &Address, key: &Bytes32) -> Bytes32 {
        println!("get(address: {:?}, key: {:?})", address, key);
        self.storage
            .get(address)
            .and_then(|map| map.get(key))
            .map(|value| value.data.clone())
            .unwrap_or_default()
    }

    fn set_storage(&mut self, address: Address, key: Bytes32, value: Bytes32) -> StorageStatus {
        println!(
            "set(address: {:?}, key: {:?}), value: {:?}",
            address, key, value
        );
        let val = self
            .storage
            .entry(address)
            .or_default()
            .entry(key)
            .or_insert_with(|| Value::new(value.clone()));
        let changed = val.update_data(value);

        match (val.modified, changed) {
            (0, true) => unreachable!(),
            (0, false) => StorageStatus::EVMC_STORAGE_ADDED,
            (1, true) => StorageStatus::EVMC_STORAGE_MODIFIED,
            (_, true) => StorageStatus::EVMC_STORAGE_MODIFIED_AGAIN,
            (_, false) => StorageStatus::EVMC_STORAGE_UNCHANGED,
        }
    }

    fn get_balance(&mut self, address: &Address) -> Uint256 {
        println!("get_balance(address: {:?})", address);
        Uint256::default()
    }

    fn call(&mut self, message: ExecutionMessage) -> ExecutionResult {
        println!("call(message: {:?})", message);
        let code = hex::decode(&SIMPLE_STORAGE_CODE).unwrap();
        let host_context = Box::new(TestHostContext::new(message.depth as u32 + 1));
        let host_context_ptr = HostContextPtr::from(host_context);
        let mut context = ExecutionContext::new(TestHostContext::interface(), host_context_ptr.ptr);
        let instance = Evmone::new();
        instance.execute(Revision::EVMC_PETERSBURG, &code, &message, &mut context)
    }

    fn selfdestruct(&mut self, address: &Address, beneficiary: &Address) {
        println!(
            "emit_log(address: {:?}, beneficiary: {:?})",
            address, beneficiary
        );
    }

    fn emit_log(&mut self, address: &Address, data: &[u8], topics: &[Bytes32]) {
        println!(
            "emit_log(address: {:?}, data: {:?}, topics: {:?})",
            address, data, topics
        );
    }

    fn copy_code(&mut self, address: &Address, code_offset: usize, buffer: &[u8]) -> usize {
        println!(
            "copy_code(address: {:?}, code_offset: {:?}, buffer: {:?})",
            address, code_offset, buffer
        );
        0
    }

    fn get_code_size(&mut self, address: &Address) -> usize {
        println!("get_code_size(address: {:?})", address);
        0
    }

    fn get_code_hash(&mut self, address: &Address) -> Bytes32 {
        println!("get_code_hash(address: {:?})", address);
        Bytes32::default()
    }

    fn get_block_hash(&mut self, number: u64) -> Bytes32 {
        println!("get_block_hash(number: {:?})", number);
        Bytes32::default()
    }
}
