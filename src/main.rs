// use libc;
use evmc_vm::ffi;

mod evmone;

use evmone::{
    get_dummy_tx_context, Evmone, ExecutionContext, ExecutionMessage, HostContext, HostContextPtr,
};

fn main() {
    for i in 0..3 {
        let interface = ffi::evmc_host_interface {
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
        let host_context_ptr = HostContextPtr::from(host_context);
        let mut context = ExecutionContext::new(interface, host_context_ptr.ptr);
        let instance = Evmone::new();

        let destination = ffi::evmc_address { bytes: [32u8; 20] };
        let sender = ffi::evmc_address { bytes: [128u8; 20] };
        let value = ffi::evmc_uint256be { bytes: [0u8; 32] };
        let create2_salt = ffi::evmc_bytes32 { bytes: [255u8; 32] };
        let raw_message = ffi::evmc_message {
            kind: ffi::evmc_call_kind::EVMC_CALL,
            flags: 44,
            depth: 66,
            gas: 4466,
            destination,
            sender,
            input_data: std::ptr::null(),
            input_size: 0,
            value,
            create2_salt,
        };
        let message = ExecutionMessage::from(raw_message);
        let code = [0u8; 0];
        let result = instance.execute(
            ffi::evmc_revision::EVMC_PETERSBURG,
            &code,
            &message,
            &mut context,
        );
        println!("[Round {}] Execution result: {:?}", i, result);
    }
}
