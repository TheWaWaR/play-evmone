// use libc;
use std::alloc::{dealloc, Layout};
use std::collections::HashMap;
use std::ptr;

use evmc_vm::{
    self as vm, ffi, EvmcContainer, EvmcVm, ExecutionContext, ExecutionMessage, ExecutionResult,
};

#[link(name = "evmone")]
extern "C" {
    fn evmc_create_evmone() -> *mut ffi::evmc_vm;
}

fn main() {
    for i in 0..3 {
        let host = ffi::evmc_host_interface {
            account_exists: None,
            get_storage: None,
            set_storage: None,
            get_balance: None,
            get_code_size: None,
            get_code_hash: None,
            copy_code: None,
            selfdestruct: None,
            call: None,
            get_tx_context: Some(get_dummy_tx_context),
            get_block_hash: None,
            emit_log: None,
        };
        let host_context = Box::new(HostContext::default());
        let host_context_ptr = Box::into_raw(host_context) as *mut ffi::evmc_host_context;
        let mut context = ExecutionContext::new(&host, host_context_ptr);
        let instance = unsafe {
            let evmone = evmc_create_evmone();
            ffi::evmc_vm {
                abi_version: (*evmone).abi_version,
                name: (*evmone).name,
                version: (*evmone).version,
                destroy: (*evmone).destroy,
                execute: (*evmone).execute,
                get_capabilities: (*evmone).get_capabilities,
                set_option: (*evmone).set_option,
            }
        };
        let container = EvmcContainer::<TestVm>::new(instance);

        let message = vm::ExecutionMessage::new(
            vm::MessageKind::EVMC_CALL,
            0,
            0,
            0,
            vm::Address::default(),
            vm::Address::default(),
            None,
            vm::Uint256::default(),
            vm::Bytes32::default(),
        );
        let code = [0u8; 0];
        let result = container.execute(
            vm::Revision::EVMC_PETERSBURG,
            &code,
            &message,
            Some(&mut context),
        );
        println!("[Round {}] Execution result: {:?}", i, result);

        unsafe {
            // Otherwise host_context will leak memory
            ptr::drop_in_place(host_context_ptr);
            dealloc(host_context_ptr as *mut u8, Layout::new::<HostContext>());
        }
    }
}

struct TestVm {}

impl EvmcVm for TestVm {
    fn init() -> Self {
        println!("TestVm::init");
        TestVm {}
    }

    fn execute(
        &self,
        _revision: ffi::evmc_revision,
        _code: &[u8],
        message: &ExecutionMessage,
        _context: Option<&mut ExecutionContext>,
    ) -> ExecutionResult {
        println!("TestVm.execute: {:?}", message);
        ExecutionResult::failure()
    }
}

type Bytes32 = [u8; 32];

#[derive(Default, Debug)]
pub struct HostContext {
    code: Vec<u8>,
    storage: HashMap<Bytes32, Bytes32>,
}

unsafe extern "C" fn get_dummy_tx_context(
    context: *mut ffi::evmc_host_context,
) -> ffi::evmc_tx_context {
    let ctx = Box::from_raw(context as *mut HostContext);
    println!("host context: {:?}", ctx);
    std::mem::forget(ctx);
    ffi::evmc_tx_context {
        tx_gas_price: vm::Uint256::default(),
        tx_origin: vm::Address::default(),
        block_coinbase: vm::Address::default(),
        block_number: 0,
        block_timestamp: 0,
        block_gas_limit: 0,
        block_difficulty: vm::Uint256::default(),
        chain_id: vm::Uint256::default(),
    }
}
