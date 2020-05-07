mod abi;
mod abi_cmd;
mod evmc;

use std::collections::HashMap;
use std::fmt;
use std::fs;

use clap::{App, Arg, ArgMatches, SubCommand};
use evmc::{
    get_interface, Address, Bytes32, CallKind, EvmcVm, ExecutionContext, ExecutionMessage,
    ExecutionResult, HostContext, HostContextPtr, HostContextWrapper, HostInterface, Revision,
    StatusCode, StorageStatus, TxContext, Uint256,
};
use evmc_sys as ffi;
use serde::{Deserialize, Serialize};

#[link(name = "evmone")]
extern "C" {
    fn evmc_create_evmone() -> *mut ffi::evmc_vm;
}

// TODO
// ====
//  [x]: save/load storage(TestHostContext) from a json file
//  [x]: Merge two TestHostContext
//  [x]: Test SimpleStorage::set
//  [x]: Test SimpleStorage::get
//  [x]: Test LogEvents::log
//  [x]: Test create contract
//  [ ]: Test call other contract
//  [ ]: Test selfdestruct

fn main() -> Result<(), String> {
    let arg_input_data = Arg::with_name("input-data")
        .long("input-data")
        .short("i")
        .takes_value(true)
        .help("The input data for the contract");
    let arg_input_storage = Arg::with_name("input-storage")
        .long("input-storage")
        .short("s")
        .takes_value(true)
        .help("The storage to run the contract");
    let arg_output_storage = Arg::with_name("output-storage")
        .long("output-storage")
        .short("o")
        .takes_value(true)
        .help("The storage after run the contract");
    let arg_address = Arg::with_name("address")
        .long("address")
        .takes_value(true)
        .required(true)
        .help("The account address");
    let global_matches = App::new("Play evmone")
        .subcommand(
            SubCommand::with_name("list")
                .about("List all accounts")
                .arg(arg_input_storage.clone()),
        )
        .subcommand(
            SubCommand::with_name("remove")
                .about("Remove an account")
                .arg(arg_input_storage.clone())
                .arg(arg_address.clone()),
        )
        .subcommand(
            SubCommand::with_name("show")
                .about("Show the details of an account")
                .arg(arg_input_storage.clone())
                .arg(arg_address.clone()),
        )
        .subcommand(
            SubCommand::with_name("create")
                .about("Create contract by code and input")
                .arg(
                    Arg::with_name("code")
                        .long("code")
                        .short("c")
                        .takes_value(true)
                        .required(true)
                        .help("The binary code path"),
                )
                .arg(arg_address.clone())
                .arg(
                    arg_input_data
                        .clone()
                        .help("The input data file for the contract"),
                )
                .arg(arg_input_storage.clone())
                .arg(arg_output_storage.clone()),
        )
        .subcommand(
            SubCommand::with_name("call")
                .about("Call a contract")
                .arg(arg_address.clone().required(true))
                .arg(arg_input_data.clone())
                .arg(arg_input_storage.clone().required(true))
                .arg(arg_output_storage)
                .arg(
                    Arg::with_name("static")
                        .long("static")
                        .help("Call with static mode"),
                ),
        )
        .subcommand(abi_cmd::sub_command("ethabi"))
        .get_matches();

    if let Some(sub_matches) = global_matches.subcommand_matches("ethabi") {
        return abi_cmd::process(sub_matches);
    }

    let get_context = |matches: &ArgMatches, destination, required| -> Result<_, String> {
        if required && matches.value_of("input-storage").is_none() {
            return Err("<input-storage> is required!".to_string());
        }
        let host_context: TestHostContext = matches
            .value_of("input-storage")
            .map(|path| {
                println!("Load context from: {}", path);
                let json_string = String::from_utf8(fs::read(path).unwrap()).unwrap();
                serde_json::from_slice(json_string.trim().as_bytes()).unwrap()
            })
            .unwrap_or_else(|| {
                println!("New context for: {:?}", destination);
                TestHostContext::new(0, destination)
            });
        Ok(host_context)
    };

    let vm = EvmcVm::new(unsafe { evmc_create_evmone() });
    match global_matches.subcommand() {
        ("create", Some(sub_matches)) => {
            let value = Uint256([3u8; 32]);
            let destination: Address = sub_matches
                .value_of("address")
                .map(|s| serde_json::from_str(format!("\"{}\"", s).as_str()).unwrap())
                .unwrap();
            let host_context = get_context(sub_matches, destination.clone(), false)?;
            if host_context.contract_exists(&destination) {
                return Err(format!("Contract already exists: {:?}", destination));
            }
            let host_context_ptr = HostContextPtr::from(Box::new(host_context));
            let mut context =
                ExecutionContext::new(TestHostContext::interface(), host_context_ptr.ptr);
            let code = sub_matches.value_of("code").map(load_binary).unwrap();
            let input_data = sub_matches
                .value_of("input-data")
                .map(load_binary)
                .unwrap_or_default();

            let raw_message = ffi::evmc_message {
                kind: CallKind::EVMC_CREATE,
                flags: 0,
                depth: 0,
                gas: 4_466_666_666,
                destination: destination.clone().into(),
                sender: Address([128u8; 20]).into(),
                input_data: input_data.as_ptr(),
                input_size: input_data.len(),
                value: value.into(),
                create2_salt: Bytes32([1u8; 32]).into(),
            };
            let message = ExecutionMessage::from(&raw_message);

            let result = vm.execute(Revision::EVMC_MAX_REVISION, &code, &message, &mut context);
            println!("Execution result: {:#?}\n", result);

            assert_eq!(result.create_address, Address::default());
            let mut wrapper = HostContextWrapper::from(context.context);
            let context: &mut TestHostContext = &mut wrapper;
            if result.status_code == StatusCode::EVMC_SUCCESS && message.is_create() {
                context.update_code(destination, result.output_data);
            }

            if let Some(output_storage_path) = sub_matches.value_of("output-storage") {
                let data = serde_json::to_string_pretty(context).unwrap();
                fs::write(output_storage_path, data.as_bytes()).unwrap();
            }
        }
        ("call", Some(sub_matches)) => {
            let value = Uint256([0u8; 32]);
            let destination: Address = sub_matches
                .value_of("address")
                .map(|s| serde_json::from_str(format!("\"{}\"", s).as_str()).unwrap())
                .unwrap();
            let host_context = get_context(sub_matches, destination.clone(), true)?;
            let code = host_context
                .accounts
                .get(&destination)
                .unwrap()
                .code
                .clone()
                .unwrap();
            let host_context_ptr = HostContextPtr::from(Box::new(host_context));
            let mut context =
                ExecutionContext::new(TestHostContext::interface(), host_context_ptr.ptr);

            let input_data = sub_matches
                .value_of("input-data")
                .map(|s| hex::decode(s).unwrap())
                .unwrap_or_default();

            println!("address: {:?}", destination);
            println!("code: {}", hex::encode(&code.0));
            println!("input-data: {}", hex::encode(&input_data));
            let is_static = sub_matches.is_present("static");
            let mut flags: u32 = 0;
            unsafe {
                if is_static {
                    flags |=
                        std::mem::transmute::<ffi::evmc_flags, u32>(ffi::evmc_flags::EVMC_STATIC);
                }
            }
            let raw_message = ffi::evmc_message {
                kind: CallKind::EVMC_CALL,
                flags,
                depth: 0,
                gas: 4_400_000,
                destination: destination.clone().into(),
                sender: Address([128u8; 20]).into(),
                input_data: input_data.as_ptr(),
                input_size: input_data.len(),
                value: value.into(),
                create2_salt: Default::default(),
            };
            let message = ExecutionMessage::from(&raw_message);

            let result = vm.execute(Revision::EVMC_MAX_REVISION, &code.0, &message, &mut context);
            println!("Execution result: {:#?}\n", result);

            assert_eq!(result.create_address, Address::default());
            let mut wrapper = HostContextWrapper::from(context.context);
            let context: &mut TestHostContext = &mut wrapper;
            if result.status_code == StatusCode::EVMC_SUCCESS
                && (message.kind == CallKind::EVMC_CREATE || message.kind == CallKind::EVMC_CREATE2)
            {
                context.update_code(destination, result.output_data);
            }

            if let Some(output_storage_path) = sub_matches.value_of("output-storage") {
                let data = serde_json::to_string_pretty(context).unwrap();
                fs::write(output_storage_path, data.as_bytes()).unwrap();
            }
        }
        ("list", Some(sub_matches)) => {
            let host_context = get_context(sub_matches, Default::default(), true)?;
            for (address, account) in host_context.accounts {
                println!(
                    "Account(address: {:?}, code: {:?}, nonce: {})",
                    address,
                    account.code.is_some(),
                    account.nonce,
                );
            }
        }
        ("show", Some(sub_matches)) => {
            let host_context = get_context(sub_matches, Default::default(), true)?;
            let destination: Address = sub_matches
                .value_of("address")
                .map(|s| serde_json::from_str(format!("\"{}\"", s).as_str()).unwrap())
                .unwrap();
            if let Some(account) = host_context.accounts.get(&destination) {
                println!("{}", serde_json::to_string_pretty(account).unwrap());
            } else {
                return Err(format!("Account not exists: {:?}", destination));
            }
        }
        ("remove", Some(sub_matches)) => {
            let mut host_context = get_context(sub_matches, Default::default(), true)?;
            let destination: Address = sub_matches
                .value_of("address")
                .map(|s| serde_json::from_str(format!("\"{}\"", s).as_str()).unwrap())
                .unwrap();
            if let Some(account) = host_context.accounts.remove(&destination) {
                let path = sub_matches.value_of("input-storage").unwrap();
                let data = serde_json::to_string_pretty(&host_context).unwrap();
                fs::write(path, data.as_bytes()).unwrap();
                println!(
                    "[Account removed]: {:?}\n{}",
                    destination,
                    serde_json::to_string_pretty(&account).unwrap(),
                );
            } else {
                return Err(format!("Account not exists: {:?}", destination));
            }
        }
        (name, args) => {
            println!("ERROR subcommand: name={}, args: {:?}", name, args);
        }
    }

    Ok(())
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

#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct JsonBytes(pub Vec<u8>);

fn parse_bytes(bytes: &[u8]) -> Result<JsonBytes, String> {
    let mut target = vec![0u8; bytes.len() / 2];
    hex::decode_to_slice(bytes, &mut target).map_err(|e| e.to_string())?;
    Ok(JsonBytes(target))
}
impl_serde!(JsonBytes, BytesVisitor, parse_bytes);

impl fmt::Debug for JsonBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Default, Deserialize, Serialize)]
pub struct Value {
    data: Bytes32,
    // Modify time:
    //   0 => first set
    //   1 => modified
    //   2..n => modifled again
    modify_time: usize,
}

impl Value {
    fn new(data: Bytes32) -> Value {
        Value {
            data,
            modify_time: 0,
        }
    }

    fn update_data(&mut self, data: Bytes32) -> bool {
        if data != self.data {
            self.data = data;
            self.modify_time += 1;
            true
        } else {
            false
        }
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct LogEntry {
    data: JsonBytes,
    topics: Vec<Bytes32>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct AccountData {
    nonce: u64,
    address: Address,
    // The code stored in the account, not the code created the account
    code: Option<JsonBytes>,
    storage: HashMap<Bytes32, Value>,
    logs: Vec<LogEntry>,
}

impl AccountData {
    pub fn new(address: Address) -> AccountData {
        println!("AccountData::new({:?})", address);
        AccountData {
            nonce: 0,
            address,
            code: None,
            storage: HashMap::default(),
            logs: Vec::new(),
        }
    }

    fn nonce_u256(&self) -> Uint256 {
        let mut data = [0u8; 32];
        data[0..8].copy_from_slice(&self.nonce.to_le_bytes());
        Uint256(data)
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct TestHostContext {
    pub depth: u32,
    // Current account's address
    pub current_account: Address,
    pub accounts: HashMap<Address, AccountData>,
    pub destructed_accounts: Vec<Address>,
}

impl TestHostContext {
    pub fn new(depth: u32, current_account: Address) -> TestHostContext {
        TestHostContext {
            depth,
            current_account,
            accounts: HashMap::default(),
            destructed_accounts: Vec::new(),
        }
    }

    pub fn contract_exists(&self, address: &Address) -> bool {
        self.accounts
            .get(address)
            .map(|account| account.code.is_some())
            .unwrap_or(false)
    }

    pub fn update_code(&mut self, address: Address, code: Vec<u8>) {
        // println!(">> before update_code context: {:#?}", self);
        let account = self
            .accounts
            .entry(address.clone())
            .or_insert_with(|| AccountData::new(address));
        account.code = Some(JsonBytes(code));
        // println!(">> after update_code context: {:#?}", self);
    }

    // We assume the `other` account always have latest state
    pub fn update(&mut self, other: &TestHostContext) {
        if other.destructed_accounts.len() < self.destructed_accounts.len() {
            panic!(
                "other destructed_accounts length invalid ({} < {})",
                other.destructed_accounts.len(),
                self.destructed_accounts.len()
            );
        }
        for address in &other.destructed_accounts {
            if other.accounts.contains_key(address) {
                panic!(
                    "Invalid state for context, address={:?}",
                    other.current_account
                );
            }
        }

        self.accounts = other.accounts.clone();
        self.destructed_accounts = other.destructed_accounts.clone();
    }
}

impl HostContext for TestHostContext {
    fn interface() -> HostInterface {
        get_interface::<TestHostContext>()
    }

    fn get_tx_context(&mut self) -> TxContext {
        println!("get_tx_context()");
        TxContext {
            tx_gas_price: Uint256::default().into(),
            tx_origin: Address([128u8; 20]).into(),
            block_coinbase: Address::default().into(),
            block_number: 1,
            block_timestamp: 1,
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
        self.accounts
            .get(address)
            .and_then(|account| account.storage.get(key))
            .map(|value| value.data.clone())
            .unwrap_or_default()
    }

    fn set_storage(&mut self, address: Address, key: Bytes32, value: Bytes32) -> StorageStatus {
        // println!(">> before set_storage context: {:#?}", self);
        println!(
            "set(address: {:?}, key: {:?}), value: {:?}, contains_address: {}",
            address,
            key,
            value,
            self.accounts.contains_key(&address)
        );
        let (modify_time, changed) = {
            let val = self
                .accounts
                .entry(address.clone())
                .or_insert_with(|| AccountData::new(address))
                .storage
                .entry(key)
                .or_insert_with(|| Value::new(value.clone()));
            let changed = val.update_data(value);
            (val.modify_time, changed)
        };
        // println!(">> after set_storage context: {:#?}", self);

        match (modify_time, changed) {
            (0, true) => panic!("Invalid storage value data"),
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
        let sender = Address::from(message.inner.sender);
        let sender_nonce = self
            .accounts
            .entry(sender.clone())
            .or_insert_with(|| AccountData::new(sender))
            .nonce_u256();
        let (destination, _code_hash) = message.destination(sender_nonce);
        let mut message_inner = *message.inner;
        let message = {
            message_inner.destination = destination.clone().into();
            ExecutionMessage {
                inner: &message_inner,
            }
        };
        println!("call destination: {:?}", destination);
        let code = if message.is_create() {
            message.input_data().to_vec()
        } else if let Some(account) = self.accounts.get(&destination) {
            if let Some(code) = account.code.as_ref() {
                code.clone().0
            } else {
                panic!("No code found form account: {:?}", destination);
            }
        } else {
            panic!("Not such account: {:?}", destination);
        };
        let host_context = {
            let mut context = self.clone();
            context.depth = message.depth as u32 + 1;
            context.current_account = destination.clone();
            Box::new(context)
        };
        let host_context_ptr = HostContextPtr::from(host_context);
        let mut context = ExecutionContext::new(TestHostContext::interface(), host_context_ptr.ptr);
        let vm = EvmcVm::new(unsafe { evmc_create_evmone() });
        let mut result = vm.execute(Revision::EVMC_PETERSBURG, &code, &message, &mut context);
        println!("Execution result: {:#?}\n", result);

        let mut wrapper = HostContextWrapper::from(context.context);
        let context: &mut TestHostContext = &mut wrapper;
        if result.status_code == StatusCode::EVMC_SUCCESS && message.is_create() {
            context.update_code(destination.clone(), result.output_data.clone());
        }
        result.create_address = destination;
        self.update(context);
        result
    }

    fn selfdestruct(&mut self, address: &Address, beneficiary: &Address) {
        self.destructed_accounts.push(address.clone());
        println!(
            "emit_log(address: {:?}, beneficiary: {:?})",
            address, beneficiary
        );
    }

    fn emit_log(&mut self, address: &Address, data: &[u8], topics: &[Bytes32]) {
        println!(
            "emit_log(address: {:?}, data: {}, topics: {:?})",
            address,
            hex::encode(data),
            topics
        );
        self.accounts
            .entry(address.clone())
            .or_insert_with(|| AccountData::new(address.clone()))
            .logs
            .push(LogEntry {
                data: JsonBytes(data.to_vec()),
                topics: topics.to_vec(),
            });
    }

    fn copy_code(&mut self, address: &Address, code_offset: usize, buffer: &[u8]) -> usize {
        println!(
            "copy_code(address: {:?}, code_offset: {:?}, buffer: {})",
            address,
            code_offset,
            hex::encode(buffer)
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
