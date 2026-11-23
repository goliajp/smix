//! The macOS side of [`crate::keyboard`]: SimulatorKit's HID client for a
//! booted device, and its one-key message.

use crate::posture_sys::{Id, booted_device, developer_dir, msg0, sel};
use std::ffi::{CString, c_char, c_void};

unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn objc_msgSend();
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

const CLIENT: &str = "_TtC12SimulatorKit24SimDeviceLegacyHIDClient";
const MESSAGE: &str = "IndigoHIDMessageForKeyboardArbitrary";

type MessageFn = unsafe extern "C" fn(u32, u32) -> *mut c_void;

/// A booted simulator's keyboard.
pub(crate) struct Keyboard {
    client: Id,
    message: MessageFn,
}

fn c(s: &str) -> CString {
    CString::new(s).expect("no NUL")
}

/// Xcode 27 keeps SimulatorKit under `Contents/SharedFrameworks`, earlier
/// ones under the developer directory.
fn simulator_kit() -> Result<*mut c_void, String> {
    let dev = developer_dir()?;
    let contents = dev.strip_suffix("/Developer").unwrap_or(&dev);
    let tried = [
        format!("{contents}/SharedFrameworks/SimulatorKit.framework/SimulatorKit"),
        format!("{dev}/Library/PrivateFrameworks/SimulatorKit.framework/SimulatorKit"),
    ];
    for path in &tried {
        let handle = unsafe { dlopen(c(path).as_ptr(), 2) };
        if !handle.is_null() {
            return Ok(handle);
        }
    }
    Err(format!(
        "SimulatorKit could not be loaded from {}",
        tried.join(" or ")
    ))
}

impl Keyboard {
    pub(crate) fn open(udid: &str) -> Result<Self, String> {
        // CoreSimulator first: SimulatorKit's client takes its device
        let device = booted_device(udid)?;
        let kit = simulator_kit()?;
        let message = unsafe { dlsym(kit, c(MESSAGE).as_ptr()) };
        if message.is_null() {
            return Err(format!("SimulatorKit has no {MESSAGE}"));
        }
        let class = unsafe { objc_getClass(c(CLIENT).as_ptr()) };
        if class.is_null() {
            return Err(format!("SimulatorKit has no {CLIENT}"));
        }
        let init: unsafe extern "C" fn(Id, *mut c_void, Id, *mut Id) -> Id =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        let mut err: Id = std::ptr::null_mut();
        let client = unsafe {
            init(
                msg0(class, "alloc"),
                sel("initWithDevice:error:"),
                device,
                &mut err,
            )
        };
        if client.is_null() {
            return Err(format!("SimulatorKit gave no keyboard for {udid}"));
        }
        Ok(Self {
            client,
            message: unsafe { std::mem::transmute::<*mut c_void, MessageFn>(message) },
        })
    }

    /// One key going down or coming up, by USB HID usage.
    pub(crate) fn send(&self, usage: u32, down: bool) -> Result<(), String> {
        let message = unsafe { (self.message)(usage, if down { 1 } else { 2 }) };
        if message.is_null() {
            return Err(format!("SimulatorKit made no message for key {usage:#x}"));
        }
        // freeWhenDone: the client frees the message it was handed
        let send: unsafe extern "C" fn(Id, *mut c_void, *mut c_void, bool, Id, Id) =
            unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
        unsafe {
            send(
                self.client,
                sel("sendWithMessage:freeWhenDone:completionQueue:completion:"),
                message,
                true,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        Ok(())
    }
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        msg0(self.client, "release");
    }
}

// the client is used from one task at a time and holds no thread's state
unsafe impl Send for Keyboard {}
unsafe impl Sync for Keyboard {}
