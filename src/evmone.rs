use std::alloc::{dealloc, Layout};
use std::collections::HashMap;
use std::ops::Deref;
use std::ptr;

use evmc_vm::ffi;

#[link(name = "evmone")]
extern "C" {
    fn evmc_create_evmone() -> *mut ffi::evmc_vm;
}

pub struct Evmone {
    pub inner: *mut ffi::evmc_vm,
}

impl Evmone {
    pub fn new() -> Evmone {
        let inner = unsafe { evmc_create_evmone() };
        Evmone { inner }
    }

    pub fn execute(
        &self,
        revision: ffi::evmc_revision,
        code: &[u8],
        message: &ExecutionMessage,
        context: &mut ExecutionContext,
    ) -> ExecutionResult {
        let result = unsafe {
            let execute_fn = (*self.inner).execute.clone().unwrap();
            execute_fn(
                self.inner,
                context.const_interface(),
                context.context,
                revision,
                message.as_ptr(),
                code.as_ptr(),
                code.len(),
            )
        };
        ExecutionResult::from(result)
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Bytes32(pub [u8; 32]);

#[derive(Debug)]
pub struct ExecutionResult {
    inner: ffi::evmc_result,
}

impl From<ffi::evmc_result> for ExecutionResult {
    fn from(result: ffi::evmc_result) -> ExecutionResult {
        ExecutionResult { inner: result }
    }
}

pub struct ExecutionContext {
    pub interface: ffi::evmc_host_interface,
    pub context: *mut ffi::evmc_host_context,
    pub tx_context: ffi::evmc_tx_context,
}

impl ExecutionContext {
    pub fn new(
        interface: ffi::evmc_host_interface,
        context: *mut ffi::evmc_host_context,
    ) -> ExecutionContext {
        let tx_context = unsafe {
            assert!(interface.get_tx_context.is_some());
            interface.get_tx_context.unwrap()(context)
        };
        ExecutionContext {
            interface,
            context,
            tx_context,
        }
    }

    pub fn const_interface(&self) -> *const ffi::evmc_host_interface {
        (&self.interface) as *const ffi::evmc_host_interface
    }
}

pub struct ExecutionMessage {
    pub inner: ffi::evmc_message,
}

impl From<ffi::evmc_message> for ExecutionMessage {
    fn from(inner: ffi::evmc_message) -> ExecutionMessage {
        ExecutionMessage { inner }
    }
}

impl ExecutionMessage {
    pub fn as_ptr(&self) -> *const ffi::evmc_message {
        (&self.inner) as *const ffi::evmc_message
    }
}

#[derive(Default, Debug)]
pub struct HostContext {
    pub code: Vec<u8>,
    pub storage: HashMap<Bytes32, Bytes32>,
}

pub struct HostContextPtr {
    pub ptr: *mut ffi::evmc_host_context,
}

impl Drop for HostContextPtr {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.ptr);
            dealloc(self.ptr as *mut u8, Layout::new::<HostContext>());
        }
    }
}

impl From<Box<HostContext>> for HostContextPtr {
    fn from(ctx: Box<HostContext>) -> HostContextPtr {
        let ptr = Box::into_raw(ctx) as *mut ffi::evmc_host_context;
        HostContextPtr { ptr }
    }
}

pub struct HostContextWrapper {
    inner: Option<Box<HostContext>>,
}

impl Drop for HostContextWrapper {
    fn drop(&mut self) {
        std::mem::forget(self.inner.take().unwrap());
    }
}

impl From<*mut ffi::evmc_host_context> for HostContextWrapper {
    fn from(ptr: *mut ffi::evmc_host_context) -> HostContextWrapper {
        let inner = Some(unsafe { Box::from_raw(ptr as *mut HostContext) });
        HostContextWrapper { inner }
    }
}

impl Deref for HostContextWrapper {
    type Target = Box<HostContext>;
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

pub unsafe extern "C" fn get_dummy_tx_context(
    context: *mut ffi::evmc_host_context,
) -> ffi::evmc_tx_context {
    let ctx = Box::from_raw(context as *mut HostContext);
    println!("host context: {:?}", ctx);
    std::mem::forget(ctx);
    ffi::evmc_tx_context {
        tx_gas_price: ffi::evmc_uint256be::default(),
        tx_origin: ffi::evmc_address::default(),
        block_coinbase: ffi::evmc_address::default(),
        block_number: 0,
        block_timestamp: 0,
        block_gas_limit: 0,
        block_difficulty: ffi::evmc_uint256be::default(),
        chain_id: ffi::evmc_uint256be::default(),
    }
}
