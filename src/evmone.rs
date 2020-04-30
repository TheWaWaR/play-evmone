use std::alloc::{dealloc, Layout};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::slice::from_raw_parts;

use evmc_sys as ffi;

/// EVMC call kind.
pub type CallKind = ffi::evmc_call_kind;

/// EVMC message (call) flags.
pub type CallFlags = ffi::evmc_flags;

/// EVMC status code.
pub type StatusCode = ffi::evmc_status_code;

/// EVMC storage status.
pub type StorageStatus = ffi::evmc_storage_status;

/// EVMC VM revision.
pub type Revision = ffi::evmc_revision;

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
        revision: Revision,
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

macro_rules! impl_convert {
    ($struct:ident, $inner:ty, $target:path) => {
        #[derive(Eq, PartialEq, Hash, Default, Clone)]
        pub struct $struct(pub $inner);

        impl From<$target> for $struct {
            fn from(data: $target) -> $struct {
                $struct(data.bytes)
            }
        }
        impl From<$struct> for $target {
            fn from(data: $struct) -> $target {
                $target { bytes: data.0 }
            }
        }
        impl ::std::fmt::Debug for $struct {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> Result<(), ::std::fmt::Error> {
                let prefix = if f.alternate() { "0x" } else { "" };
                write!(f, "{}{}", prefix, ::hex::encode(self.0))
            }
        }
    };
}

impl_convert!(Address, [u8; 20], ffi::evmc_address);
impl_convert!(Bytes32, [u8; 32], ffi::evmc_bytes32);
// Big Endian
impl_convert!(Uint256, [u8; 32], ffi::evmc_uint256be);

#[derive(Debug)]
pub struct ExecutionResult {
    inner: ffi::evmc_result,
}

impl From<ffi::evmc_result> for ExecutionResult {
    fn from(result: ffi::evmc_result) -> ExecutionResult {
        ExecutionResult { inner: result }
    }
}
impl From<ExecutionResult> for ffi::evmc_result {
    fn from(result: ExecutionResult) -> ffi::evmc_result {
        result.inner
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

#[derive(Debug)]
pub struct ExecutionMessage<'a> {
    pub inner: &'a ffi::evmc_message,
}

impl<'a> From<&'a ffi::evmc_message> for ExecutionMessage<'a> {
    fn from(inner: &'a ffi::evmc_message) -> ExecutionMessage<'a> {
        ExecutionMessage { inner }
    }
}

impl<'a> Deref for ExecutionMessage<'a> {
    type Target = ffi::evmc_message;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<'a> ExecutionMessage<'a> {
    pub fn as_ptr(&self) -> *const ffi::evmc_message {
        self.inner as *const ffi::evmc_message
    }
}

pub struct HostContextPtr<T> {
    pub ptr: *mut ffi::evmc_host_context,
    _data: PhantomData<T>,
}

impl<T: Sized> Drop for HostContextPtr<T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.ptr);
            dealloc(self.ptr as *mut u8, Layout::new::<T>());
        }
    }
}

impl<T: HostContext + Sized> From<Box<T>> for HostContextPtr<T> {
    fn from(ctx: Box<T>) -> HostContextPtr<T> {
        let ptr = Box::into_raw(ctx) as *mut ffi::evmc_host_context;
        HostContextPtr {
            ptr,
            _data: PhantomData,
        }
    }
}

pub struct HostContextWrapper<T> {
    inner: Option<Box<T>>,
}

impl<T> Drop for HostContextWrapper<T> {
    fn drop(&mut self) {
        std::mem::forget(self.inner.take().unwrap());
    }
}

impl<T: HostContext> From<*mut ffi::evmc_host_context> for HostContextWrapper<T> {
    fn from(ptr: *mut ffi::evmc_host_context) -> HostContextWrapper<T> {
        let inner = Some(unsafe { Box::from_raw(ptr as *mut T) });
        HostContextWrapper { inner }
    }
}

impl<T: HostContext> Deref for HostContextWrapper<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().unwrap()
    }
}

impl<T: HostContext> DerefMut for HostContextWrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().unwrap()
    }
}

pub trait HostContext {
    fn interface() -> ffi::evmc_host_interface;

    fn get_tx_context(&mut self) -> ffi::evmc_tx_context;
    fn account_exists(&mut self, address: &Address) -> bool;
    fn get_storage(&mut self, address: &Address, key: &Bytes32) -> Bytes32;
    fn set_storage(&mut self, address: Address, key: Bytes32, value: Bytes32) -> StorageStatus;
    fn get_balance(&mut self, address: &Address) -> Uint256;
    fn call(&mut self, msg: ExecutionMessage) -> ExecutionResult;
    fn selfdestruct(&mut self, address: &Address, beneficiary: &Address);
    fn emit_log(&mut self, address: &Address, data: &[u8], topics: &[ffi::evmc_bytes32]);
    fn copy_code(&mut self, address: &Address, code_offset: usize, buffer: &[u8]) -> usize;
    fn get_code_size(&mut self, address: &Address) -> usize;
    fn get_code_hash(&mut self, address: &Address) -> Bytes32;
    fn get_block_hash(&mut self, number: u64) -> Bytes32;
}

pub fn get_interface<T: HostContext>() -> ffi::evmc_host_interface {
    unsafe extern "C" fn account_exists<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
    ) -> bool {
        let address = Address::from(*address);
        HostContextWrapper::<T>::from(context).account_exists(&address)
    }

    unsafe extern "C" fn get_tx_context<T: HostContext>(
        context: *mut ffi::evmc_host_context,
    ) -> ffi::evmc_tx_context {
        HostContextWrapper::<T>::from(context).get_tx_context()
    }

    unsafe extern "C" fn get_storage<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
        key: *const ffi::evmc_bytes32,
    ) -> ffi::evmc_bytes32 {
        let address = Address::from(*address);
        let key = Bytes32::from(*key);
        HostContextWrapper::<T>::from(context)
            .get_storage(&address, &key)
            .into()
    }

    unsafe extern "C" fn set_storage<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
        key: *const ffi::evmc_bytes32,
        value: *const ffi::evmc_bytes32,
    ) -> ffi::evmc_storage_status {
        let address = Address::from(*address);
        let key = Bytes32::from(*key);
        let value = Bytes32::from(*value);
        HostContextWrapper::<T>::from(context).set_storage(address, key, value)
    }

    unsafe extern "C" fn get_balance<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
    ) -> ffi::evmc_uint256be {
        let address = Address::from(*address);
        HostContextWrapper::<T>::from(context)
            .get_balance(&address)
            .into()
    }

    unsafe extern "C" fn call<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        msg: *const ffi::evmc_message,
    ) -> ffi::evmc_result {
        let message = ExecutionMessage::from(&*msg);
        HostContextWrapper::<T>::from(context).call(message).into()
    }

    unsafe extern "C" fn emit_log<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
        data: *const u8,
        data_size: usize,
        topics: *const ffi::evmc_bytes32,
        topics_count: usize,
    ) {
        let address = Address::from(*address);
        let data: &[u8] = from_raw_parts(data, data_size);
        let topics: &[ffi::evmc_bytes32] = from_raw_parts(topics, topics_count);
        HostContextWrapper::<T>::from(context).emit_log(&address, data, topics)
    }

    unsafe extern "C" fn get_code_size<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
    ) -> usize {
        let address = Address::from(*address);
        HostContextWrapper::<T>::from(context).get_code_size(&address)
    }

    unsafe extern "C" fn get_code_hash<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
    ) -> ffi::evmc_bytes32 {
        let address = Address::from(*address);
        HostContextWrapper::<T>::from(context)
            .get_code_hash(&address)
            .into()
    }

    unsafe extern "C" fn get_block_hash<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        number: i64,
    ) -> ffi::evmc_bytes32 {
        HostContextWrapper::<T>::from(context)
            .get_block_hash(number as u64)
            .into()
    }

    unsafe extern "C" fn selfdestruct<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
        beneficiary: *const ffi::evmc_address,
    ) {
        let address = Address::from(*address);
        let beneficiary = Address::from(*beneficiary);
        HostContextWrapper::<T>::from(context).selfdestruct(&address, &beneficiary);
    }

    unsafe extern "C" fn copy_code<T: HostContext>(
        context: *mut ffi::evmc_host_context,
        address: *const ffi::evmc_address,
        code_offset: usize,
        buffer_data: *mut u8,
        buffer_size: usize,
    ) -> usize {
        let address = Address::from(*address);
        let buffer: &[u8] = from_raw_parts(buffer_data, buffer_size);
        HostContextWrapper::<T>::from(context).copy_code(&address, code_offset, buffer)
    }

    ffi::evmc_host_interface {
        get_tx_context: Some(get_tx_context::<T>),
        account_exists: Some(account_exists::<T>),
        get_storage: Some(get_storage::<T>),
        set_storage: Some(set_storage::<T>),
        get_balance: Some(get_balance::<T>),
        selfdestruct: Some(selfdestruct::<T>),
        call: Some(call::<T>),
        emit_log: Some(emit_log::<T>),
        copy_code: Some(copy_code::<T>),
        get_code_size: Some(get_code_size::<T>),
        get_code_hash: Some(get_code_hash::<T>),
        get_block_hash: Some(get_block_hash::<T>),
    }
}
