// ── PCAN-USB FFI wrapper via libloading ──
// Uses PCANBasic.dll (Windows) loaded dynamically at runtime.

use libloading::{Library, Symbol};
use std::fmt;

// ── PCAN Constants ──

pub const PCAN_USBBUS1: u16 = 0x51;
pub const PCAN_BAUD_500K: u16 = 0x001C;
pub const PCAN_MESSAGE_EXTENDED: u8 = 0x02;
pub const PCAN_ERROR_OK: u32 = 0x00000;
pub const PCAN_ERROR_QRCVEMPTY: u32 = 0x00020;

// ── PCAN Structures (C ABI) ──

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TPCANMsg {
    pub id: u32,
    pub msg_type: u8,
    pub len: u8,
    pub data: [u8; 8],
}

impl Default for TPCANMsg {
    fn default() -> Self {
        Self {
            id: 0,
            msg_type: 0,
            len: 0,
            data: [0u8; 8],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct TPCANTimestamp {
    pub millis: u32,
    pub millis_overflow: u16,
    pub micros: u16,
}

// ── Function pointer types ──

type FnInitialize = unsafe extern "system" fn(u16, u16, u8, u32, u32) -> u32;
type FnUninitialize = unsafe extern "system" fn(u16) -> u32;
type FnRead = unsafe extern "system" fn(u16, *mut TPCANMsg, *mut TPCANTimestamp) -> u32;
type FnWrite = unsafe extern "system" fn(u16, *mut TPCANMsg) -> u32;

// ── PcanDriver ──

pub struct PcanDriver {
    _lib: Library, // kept alive so function pointers remain valid
    channel: u16,
    fn_read: FnRead,
    fn_write: FnWrite,
    fn_uninit: FnUninitialize,
}

// Safety: Library is kept alive solely to prevent unloading — it is never
// accessed after init. The function pointers are Copy and point to thread-safe
// PCAN API functions (PEAK documents CAN_Read/CAN_Write as thread-safe).
unsafe impl Send for PcanDriver {}
unsafe impl Sync for PcanDriver {}

#[derive(Debug)]
pub struct PcanError {
    pub code: u32,
    pub message: String,
}

impl fmt::Display for PcanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PCAN error 0x{:05X}: {}", self.code, self.message)
    }
}

impl PcanDriver {
    /// Load PCANBasic.dll, initialize the given channel at the given baudrate.
    pub fn new(channel: u16, baudrate: u16) -> Result<Self, PcanError> {
        let lib = unsafe { Library::new("PCANBasic.dll") }.map_err(|e| PcanError {
            code: 0xFFFFFFFF,
            message: format!("Nie można załadować PCANBasic.dll: {}", e),
        })?;

        let fn_init: FnInitialize;
        let fn_uninit: FnUninitialize;
        let fn_read: FnRead;
        let fn_write: FnWrite;

        unsafe {
            fn_init = **lib
                .get::<Symbol<FnInitialize>>(b"CAN_Initialize\0")
                .map_err(|e| PcanError {
                    code: 0xFFFFFFFF,
                    message: format!("Symbol CAN_Initialize not found: {}", e),
                })?;
            fn_uninit = **lib
                .get::<Symbol<FnUninitialize>>(b"CAN_Uninitialize\0")
                .map_err(|e| PcanError {
                    code: 0xFFFFFFFF,
                    message: format!("Symbol CAN_Uninitialize not found: {}", e),
                })?;
            fn_read = **lib
                .get::<Symbol<FnRead>>(b"CAN_Read\0")
                .map_err(|e| PcanError {
                    code: 0xFFFFFFFF,
                    message: format!("Symbol CAN_Read not found: {}", e),
                })?;
            fn_write = **lib
                .get::<Symbol<FnWrite>>(b"CAN_Write\0")
                .map_err(|e| PcanError {
                    code: 0xFFFFFFFF,
                    message: format!("Symbol CAN_Write not found: {}", e),
                })?;
        }

        // CAN_Initialize(channel, baudrate, hw_type=0, io_port=0, interrupt=0)
        let status = unsafe { fn_init(channel, baudrate, 0, 0, 0) };
        if status != PCAN_ERROR_OK {
            return Err(PcanError {
                code: status,
                message: format!(
                    "CAN_Initialize failed for channel 0x{:02X}, baud 0x{:04X}",
                    channel, baudrate
                ),
            });
        }

        println!(
            "[PCAN] Initialized channel 0x{:02X} @ baudrate 0x{:04X}",
            channel, baudrate
        );

        Ok(Self {
            _lib: lib,
            channel,
            fn_read,
            fn_write,
            fn_uninit,
        })
    }

    /// Read a single CAN message from the receive buffer.
    /// Returns Ok(Some(msg)) if a message was available, Ok(None) if buffer empty.
    pub fn read(&self) -> Result<Option<TPCANMsg>, u32> {
        let mut msg = TPCANMsg::default();
        let mut ts = TPCANTimestamp::default();

        let status = unsafe { (self.fn_read)(self.channel, &mut msg, &mut ts) };
        match status {
            PCAN_ERROR_OK => Ok(Some(msg)),
            PCAN_ERROR_QRCVEMPTY => Ok(None),
            err => Err(err),
        }
    }

    /// Write a CAN message with Extended Frame (29-bit ID).
    /// `id` should be the 29-bit CAN ID (already masked, without 0x80000000 flag).
    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), u32> {
        let mut msg = TPCANMsg {
            id,
            msg_type: PCAN_MESSAGE_EXTENDED,
            len: data.len().min(8) as u8,
            data: [0u8; 8],
        };
        msg.data[..data.len().min(8)].copy_from_slice(&data[..data.len().min(8)]);

        let status = unsafe { (self.fn_write)(self.channel, &mut msg) };
        if status == PCAN_ERROR_OK {
            Ok(())
        } else {
            Err(status)
        }
    }
}

impl Drop for PcanDriver {
    fn drop(&mut self) {
        let status = unsafe { (self.fn_uninit)(self.channel) };
        if status == PCAN_ERROR_OK {
            println!("[PCAN] Uninitialized channel 0x{:02X}", self.channel);
        } else {
            eprintln!(
                "[PCAN] CAN_Uninitialize failed: 0x{:05X} for channel 0x{:02X}",
                status, self.channel
            );
        }
    }
}
