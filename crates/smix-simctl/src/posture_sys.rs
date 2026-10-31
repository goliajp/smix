//! The macOS side of [`crate::posture`]: reach the simulator's HID service
//! through CoreSimulator and send it a hinge angle over XPC.

use std::ffi::{CString, c_char, c_void};

type Id = *mut c_void;
type Sel = *mut c_void;
type XpcObject = *mut c_void;
type CFTypeRef = *const c_void;

const SERVICE: &str = "com.apple.coredevice.feature.remote.hid.vendordefined";
const SIM_STATE_BOOTED: u64 = 3;
const UTF8: u32 = 0x0800_0100;
const CF_NUMBER_DOUBLE: isize = 13;
const IOCF_SERIALIZE_TO_BINARY: usize = 1;

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        alloc: CFTypeRef,
        bytes: *const u8,
        len: isize,
        encoding: u32,
        external: bool,
    ) -> CFTypeRef;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut u8, size: isize, encoding: u32) -> bool;
    fn CFNumberCreate(alloc: CFTypeRef, the_type: isize, value: *const c_void) -> CFTypeRef;
    fn CFDictionaryCreate(
        alloc: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
    fn CFDataGetLength(d: CFTypeRef) -> isize;
    fn CFDataGetBytePtr(d: CFTypeRef) -> *const u8;
    fn CFRelease(cf: CFTypeRef);
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOCFSerialize(object: CFTypeRef, options: usize) -> CFTypeRef;
}

// libxpc is part of libSystem; the `_4sim` pair is how a host process talks
// to a service inside a simulator.
unsafe extern "C" {
    fn xpc_endpoint_create_mach_port_4sim(port: u32) -> XpcObject;
    fn xpc_connection_create_from_endpoint(endpoint: XpcObject) -> XpcObject;
    fn xpc_connection_set_event_handler(conn: XpcObject, handler: *const c_void);
    fn xpc_connection_enable_sim2host_4sim(conn: XpcObject);
    fn xpc_connection_resume(conn: XpcObject);
    fn xpc_connection_send_message(conn: XpcObject, message: XpcObject);
    fn xpc_connection_cancel(conn: XpcObject);
    fn xpc_dictionary_create(keys: *const c_void, values: *const c_void, count: usize)
    -> XpcObject;
    fn xpc_dictionary_set_uint64(d: XpcObject, key: *const c_char, value: u64);
    fn xpc_dictionary_set_string(d: XpcObject, key: *const c_char, value: *const c_char);
    fn xpc_dictionary_set_data(d: XpcObject, key: *const c_char, bytes: *const c_void, len: usize);
    fn xpc_dictionary_set_value(d: XpcObject, key: *const c_char, value: XpcObject);
    fn xpc_release(object: XpcObject);
    fn mach_port_deallocate(task: u32, name: u32) -> i32;
    static mach_task_self_: u32;
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
    static _NSConcreteGlobalBlock: c_void;
}

// An XPC connection must have an event handler before it is resumed; the
// events (the peer going away when the connection is cancelled) need no
// answer, so the handler is a block that does nothing.
#[repr(C)]
struct BlockDescriptor {
    reserved: u64,
    size: u64,
}

#[repr(C)]
struct BlockLiteral {
    isa: *const c_void,
    flags: i32,
    reserved: i32,
    invoke: unsafe extern "C" fn(*const BlockLiteral, XpcObject),
    descriptor: *const BlockDescriptor,
}

struct IgnoreEvents(BlockLiteral);
// the block is immutable and global; it is only ever read
unsafe impl Sync for IgnoreEvents {}
unsafe impl Send for IgnoreEvents {}

unsafe extern "C" fn ignore_event(_block: *const BlockLiteral, _event: XpcObject) {}

static DESCRIPTOR: BlockDescriptor = BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<BlockLiteral>() as u64,
};
const BLOCK_IS_GLOBAL: i32 = 1 << 28;

fn ignore_events_block() -> *const c_void {
    static BLOCK: std::sync::OnceLock<IgnoreEvents> = std::sync::OnceLock::new();
    let block = BLOCK.get_or_init(|| {
        IgnoreEvents(BlockLiteral {
            isa: &raw const _NSConcreteGlobalBlock,
            flags: BLOCK_IS_GLOBAL,
            reserved: 0,
            invoke: ignore_event,
            descriptor: &DESCRIPTOR,
        })
    });
    &block.0 as *const BlockLiteral as *const c_void
}

fn sel(name: &str) -> Sel {
    let c = CString::new(name).expect("selector names have no NUL");
    unsafe { sel_registerName(c.as_ptr()) }
}

fn msg0(obj: Id, name: &str) -> Id {
    let f: unsafe extern "C" fn(Id, Sel) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
    unsafe { f(obj, sel(name)) }
}

fn msg_u64(obj: Id, name: &str) -> u64 {
    let f: unsafe extern "C" fn(Id, Sel) -> u64 =
        unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
    unsafe { f(obj, sel(name)) }
}

fn msg_index(obj: Id, name: &str, i: u64) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, u64) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
    unsafe { f(obj, sel(name), i) }
}

fn msg_obj_err(obj: Id, name: &str, arg: CFTypeRef) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, CFTypeRef, *mut Id) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
    let mut err: Id = std::ptr::null_mut();
    unsafe { f(obj, sel(name), arg, &mut err) }
}

fn msg_err(obj: Id, name: &str) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *mut Id) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
    let mut err: Id = std::ptr::null_mut();
    unsafe { f(obj, sel(name), &mut err) }
}

fn msg_port(obj: Id, name: &str, arg: CFTypeRef) -> u32 {
    let f: unsafe extern "C" fn(Id, Sel, CFTypeRef, *mut Id) -> u32 =
        unsafe { std::mem::transmute(objc_msgSend as unsafe extern "C" fn()) };
    let mut err: Id = std::ptr::null_mut();
    unsafe { f(obj, sel(name), arg, &mut err) }
}

/// A CF object this code owns and releases.
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}

fn cfstr(s: &str) -> Owned {
    Owned(unsafe {
        CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, UTF8, false)
    })
}

fn to_string(s: CFTypeRef) -> String {
    let mut buf = [0u8; 128];
    if s.is_null() || !unsafe { CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, UTF8) }
    {
        return String::new();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}

fn developer_dir() -> Result<String, String> {
    let out = std::process::Command::new("xcode-select")
        .arg("-p")
        .output()
        .map_err(|e| format!("xcode-select -p: {e}"))?;
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if dir.is_empty() {
        return Err("xcode-select -p named no developer directory".into());
    }
    Ok(dir)
}

/// The booted CoreSimulator device for `udid`.
fn booted_device(udid: &str) -> Result<Id, String> {
    let path =
        CString::new("/Library/Developer/PrivateFrameworks/CoreSimulator.framework/CoreSimulator")
            .expect("no NUL");
    if unsafe { dlopen(path.as_ptr(), 2) }.is_null() {
        return Err("CoreSimulator could not be loaded".into());
    }
    let class_name = CString::new("SimServiceContext").expect("no NUL");
    let class = unsafe { objc_getClass(class_name.as_ptr()) };
    if class.is_null() {
        return Err("CoreSimulator has no SimServiceContext".into());
    }
    let dir = cfstr(&developer_dir()?);
    let ctx = msg_obj_err(class, "sharedServiceContextForDeveloperDir:error:", dir.0);
    if ctx.is_null() {
        return Err("CoreSimulator gave no service context for this Xcode".into());
    }
    let set = msg_err(ctx, "defaultDeviceSetWithError:");
    if set.is_null() {
        return Err("CoreSimulator gave no device set".into());
    }
    let devices = msg0(set, "devices");
    let count = msg_u64(devices, "count");
    let wanted = udid.to_uppercase();
    for i in 0..count {
        let device = msg_index(devices, "objectAtIndex:", i);
        let uuid = msg0(device, "UDID");
        if to_string(msg0(uuid, "UUIDString") as CFTypeRef) == wanted {
            if msg_u64(device, "state") != SIM_STATE_BOOTED {
                return Err(format!("{udid} is not booted; boot it before folding it"));
            }
            return Ok(device);
        }
    }
    Err(format!("CoreSimulator lists no simulator {udid}"))
}

/// What a Device Hub control reports: the hinge slider a range in
/// degrees, the orientation picker one of a set of names.
pub enum Control<'a> {
    Hinge(f64),
    Orientation(&'a str),
}

/// The IOKit-serialised report a Device Hub control sends.
fn control_report(control: &Control<'_>) -> Result<Vec<u8>, String> {
    let keys = [
        cfstr("provider"),
        cfstr("source"),
        cfstr("type"),
        cfstr("value"),
    ];
    let (source, kind, value) = match control {
        Control::Hinge(degrees) => (
            "hinge-slider-control",
            "range",
            Owned(unsafe {
                CFNumberCreate(
                    std::ptr::null(),
                    CF_NUMBER_DOUBLE,
                    (degrees as *const f64).cast(),
                )
            }),
        ),
        Control::Orientation(name) => ("orientation-picker-control", "enum", cfstr(name)),
    };
    let values = [
        cfstr("com.apple.Virtualization.VirtualMachines"),
        cfstr(source),
        cfstr(kind),
        value,
    ];
    let key_refs: Vec<CFTypeRef> = keys.iter().map(|k| k.0).collect();
    let value_refs: Vec<CFTypeRef> = values.iter().map(|v| v.0).collect();
    let dict = Owned(unsafe {
        CFDictionaryCreate(
            std::ptr::null(),
            key_refs.as_ptr(),
            value_refs.as_ptr(),
            key_refs.len() as isize,
            &raw const kCFTypeDictionaryKeyCallBacks,
            &raw const kCFTypeDictionaryValueCallBacks,
        )
    });
    let data = Owned(unsafe { IOCFSerialize(dict.0, IOCF_SERIALIZE_TO_BINARY) });
    if data.0.is_null() {
        return Err("IOCFSerialize could not encode the control report".into());
    }
    let len = unsafe { CFDataGetLength(data.0) } as usize;
    Ok(unsafe { std::slice::from_raw_parts(CFDataGetBytePtr(data.0), len) }.to_vec())
}

/// An open connection to a simulator's HID service. The services that
/// carry a report exist only while it is open; dropping it closes it.
pub struct HidSession {
    conn: XpcObject,
}

// XPC objects may be used from any thread.
unsafe impl Send for HidSession {}

impl HidSession {
    pub fn open(udid: &str) -> Result<HidSession, String> {
        let device = booted_device(udid)?;
        let service = cfstr(SERVICE);
        let port = msg_port(device, "lookup:error:", service.0);
        if port == 0 {
            return Err(format!(
                "{udid} offers no {SERVICE}: this simulator runtime cannot be folded from outside"
            ));
        }
        let endpoint = unsafe { xpc_endpoint_create_mach_port_4sim(port) };
        unsafe { mach_port_deallocate(mach_task_self_, port) };
        if endpoint.is_null() {
            return Err("could not make an XPC endpoint for the simulator's HID service".into());
        }
        let conn = unsafe { xpc_connection_create_from_endpoint(endpoint) };
        unsafe { xpc_release(endpoint) };
        if conn.is_null() {
            return Err("could not connect to the simulator's HID service".into());
        }
        unsafe {
            xpc_connection_set_event_handler(conn, ignore_events_block());
            xpc_connection_enable_sim2host_4sim(conn);
            xpc_connection_resume(conn);
        }
        Ok(HidSession { conn })
    }

    pub fn send(&self, control: &Control<'_>) -> Result<(), String> {
        let report = control_report(control)?;
        let key = |k: &str| CString::new(k).expect("no NUL");
        unsafe {
            let payload = xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0);
            xpc_dictionary_set_uint64(payload, key("usagePage").as_ptr(), 0xff61);
            xpc_dictionary_set_uint64(payload, key("usage").as_ptr(), 0x5b);
            xpc_dictionary_set_uint64(payload, key("version").as_ptr(), 0);
            xpc_dictionary_set_data(
                payload,
                key("data").as_ptr(),
                report.as_ptr().cast(),
                report.len(),
            );
            let message = xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0);
            xpc_dictionary_set_string(
                message,
                key("featureIdentifier").as_ptr(),
                key(SERVICE).as_ptr(),
            );
            xpc_dictionary_set_string(
                message,
                key("messageType").as_ptr(),
                key("IndigoVendorDefinedEvent").as_ptr(),
            );
            xpc_dictionary_set_value(message, key("payload").as_ptr(), payload);
            xpc_connection_send_message(self.conn, message);
            xpc_release(payload);
            xpc_release(message);
        }
        Ok(())
    }
}

impl Drop for HidSession {
    fn drop(&mut self) {
        unsafe {
            xpc_connection_cancel(self.conn);
            xpc_release(self.conn);
        }
    }
}
