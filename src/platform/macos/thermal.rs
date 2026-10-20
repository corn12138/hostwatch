//! Thermal level and chip temperatures.
//!
//! The level comes from `NSProcessInfo.thermalState`, a public API: above "fair" macOS slows the
//! machine to cool it. Temperatures come from the IOHID event system's temperature sensors, which
//! has no public header; its functions are resolved at runtime and any missing piece only removes
//! the °C figures. Sensor names are Apple-silicon specific ("PMU tdie*" for CPU dies, "gas gauge
//! battery" for the battery); other machines report no temperatures rather than guessed ones.

use std::ffi::{c_char, c_void, CStr};

use objc2::rc::autoreleasepool;
use objc2_foundation::{NSProcessInfo, NSProcessInfoThermalState};

use crate::model::{Thermal, ThermalState};

type CFTypeRef = *const c_void;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFAllocatorDefault: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    fn CFRelease(value: CFTypeRef);
    fn CFRetain(value: CFTypeRef) -> CFTypeRef;
    fn CFArrayGetCount(array: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
    fn CFNumberCreate(allocator: CFTypeRef, kind: isize, value: *const c_void) -> CFTypeRef;
    fn CFDictionaryCreate(
        allocator: CFTypeRef,
        keys: *const CFTypeRef,
        values: *const CFTypeRef,
        count: isize,
        key_callbacks: *const c_void,
        value_callbacks: *const c_void,
    ) -> CFTypeRef;
    fn CFStringCreateWithCString(
        allocator: CFTypeRef,
        text: *const c_char,
        encoding: u32,
    ) -> CFTypeRef;
    fn CFStringGetCString(
        string: CFTypeRef,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> bool;
    fn CFGetTypeID(value: CFTypeRef) -> usize;
    fn CFStringGetTypeID() -> usize;
}

const UTF8: u32 = 0x0800_0100;
const SINT32: isize = 3;
/// kIOHIDEventTypeTemperature and its level field.
const TEMPERATURE_EVENT: i64 = 15;
const TEMPERATURE_FIELD: i32 = 15 << 16;

type ClientCreate = unsafe extern "C" fn(CFTypeRef) -> CFTypeRef;
type ClientSetMatching = unsafe extern "C" fn(CFTypeRef, CFTypeRef) -> i32;
type ClientCopyServices = unsafe extern "C" fn(CFTypeRef) -> CFTypeRef;
type ServiceCopyProperty = unsafe extern "C" fn(CFTypeRef, CFTypeRef) -> CFTypeRef;
type ServiceCopyEvent = unsafe extern "C" fn(CFTypeRef, i64, i32, i64) -> CFTypeRef;
type EventGetFloat = unsafe extern "C" fn(CFTypeRef, i32) -> f64;

#[derive(Clone, Copy, PartialEq)]
enum Sensor {
    CpuDie,
    Battery,
}

struct Sensors {
    client: CFTypeRef,
    services: Vec<(CFTypeRef, Sensor)>,
    copy_event: ServiceCopyEvent,
    get_float: EventGetFloat,
}

/// Reads the thermal level every sample; sensor handles are opened once and reused.
pub struct ThermalReader {
    sensors: Option<Sensors>,
}

fn symbol<T>(library: *mut c_void, name: &CStr) -> Option<T> {
    // SAFETY: `library` came from dlopen; the caller supplies the matching function type.
    let pointer = unsafe { libc::dlsym(library, name.as_ptr()) };
    (!pointer.is_null()).then(|| unsafe { std::mem::transmute_copy(&pointer) })
}

fn cf_string(text: &CStr) -> CFTypeRef {
    // SAFETY: NUL-terminated UTF-8 input; the caller releases the result.
    unsafe { CFStringCreateWithCString(kCFAllocatorDefault, text.as_ptr(), UTF8) }
}

fn string_value(value: CFTypeRef) -> Option<String> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFStringGetTypeID() } {
        return None;
    }
    let mut buffer = [0 as c_char; 128];
    // SAFETY: the buffer is valid for its length; `value` is a CFString.
    let ok = unsafe { CFStringGetCString(value, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) };
    ok.then(|| {
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

impl Sensors {
    fn open() -> Option<Self> {
        let library = unsafe {
            libc::dlopen(
                c"/System/Library/Frameworks/IOKit.framework/IOKit".as_ptr(),
                libc::RTLD_LAZY,
            )
        };
        if library.is_null() {
            return None;
        }
        let create: ClientCreate = symbol(library, c"IOHIDEventSystemClientCreate")?;
        let set_matching: ClientSetMatching =
            symbol(library, c"IOHIDEventSystemClientSetMatching")?;
        let copy_services: ClientCopyServices =
            symbol(library, c"IOHIDEventSystemClientCopyServices")?;
        let copy_property: ServiceCopyProperty =
            symbol(library, c"IOHIDServiceClientCopyProperty")?;
        let copy_event: ServiceCopyEvent = symbol(library, c"IOHIDServiceClientCopyEvent")?;
        let get_float: EventGetFloat = symbol(library, c"IOHIDEventGetFloatValue")?;
        // SAFETY: every call below follows the Core Foundation create/copy → release rule.
        unsafe {
            let client = create(kCFAllocatorDefault);
            if client.is_null() {
                return None;
            }
            // Vendor usage page 0xff00, usage 5: the temperature sensors.
            let (page, usage) = (0xff00i32, 5i32);
            let keys = [cf_string(c"PrimaryUsagePage"), cf_string(c"PrimaryUsage")];
            let values = [
                CFNumberCreate(kCFAllocatorDefault, SINT32, (&page as *const i32).cast()),
                CFNumberCreate(kCFAllocatorDefault, SINT32, (&usage as *const i32).cast()),
            ];
            let matching = CFDictionaryCreate(
                kCFAllocatorDefault,
                keys.as_ptr(),
                values.as_ptr(),
                2,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            set_matching(client, matching);
            for value in keys.into_iter().chain(values).chain([matching]) {
                CFRelease(value);
            }
            let array = copy_services(client);
            let product = cf_string(c"Product");
            let mut services = Vec::new();
            if !array.is_null() {
                for i in 0..CFArrayGetCount(array) {
                    let service = CFArrayGetValueAtIndex(array, i);
                    let name_ref = copy_property(service, product);
                    let name = string_value(name_ref);
                    if !name_ref.is_null() {
                        CFRelease(name_ref);
                    }
                    let kind = match name.as_deref() {
                        Some(n) if n.starts_with("PMU tdie") => Sensor::CpuDie,
                        Some(n) if n.starts_with("gas gauge battery") => Sensor::Battery,
                        _ => continue,
                    };
                    services.push((CFRetain(service), kind));
                }
                CFRelease(array);
            }
            CFRelease(product);
            if services.is_empty() {
                CFRelease(client);
                return None;
            }
            Some(Self {
                client,
                services,
                copy_event,
                get_float,
            })
        }
    }

    /// (mean CPU die, hottest CPU die, mean battery) in °C.
    fn read(&self) -> (Option<f64>, Option<f64>, Option<f64>) {
        let mut cpu = Vec::new();
        let mut battery = Vec::new();
        for &(service, kind) in &self.services {
            // SAFETY: `service` is retained for our lifetime; the event is released after use.
            let value = unsafe {
                let event = (self.copy_event)(service, TEMPERATURE_EVENT, 0, 0);
                if event.is_null() {
                    continue;
                }
                let value = (self.get_float)(event, TEMPERATURE_FIELD);
                CFRelease(event);
                value
            };
            // Disconnected or idle sensors report 0 or nonsense; keep plausible readings only.
            if !(1.0..=130.0).contains(&value) {
                continue;
            }
            match kind {
                Sensor::CpuDie => cpu.push(value),
                Sensor::Battery => battery.push(value),
            }
        }
        let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
        let max = cpu
            .iter()
            .copied()
            .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
        (mean(&cpu), max, mean(&battery))
    }
}

impl Drop for Sensors {
    fn drop(&mut self) {
        // SAFETY: each handle was retained exactly once in `open`.
        unsafe {
            for &(service, _) in &self.services {
                CFRelease(service);
            }
            CFRelease(self.client);
        }
    }
}

impl ThermalReader {
    pub fn new() -> Self {
        Self {
            sensors: Sensors::open(),
        }
    }

    pub fn has_sensors(&self) -> bool {
        self.sensors.is_some()
    }

    pub fn read(&self) -> Thermal {
        let state = autoreleasepool(|_| match NSProcessInfo::processInfo().thermalState() {
            NSProcessInfoThermalState::Nominal => Some(ThermalState::Nominal),
            NSProcessInfoThermalState::Fair => Some(ThermalState::Fair),
            NSProcessInfoThermalState::Serious => Some(ThermalState::Serious),
            NSProcessInfoThermalState::Critical => Some(ThermalState::Critical),
            _ => None,
        });
        let (cpu, cpu_max, battery) = self
            .sensors
            .as_ref()
            .map_or((None, None, None), Sensors::read);
        Thermal {
            state,
            cpu_celsius: cpu,
            cpu_max_celsius: cpu_max,
            battery_celsius: battery,
        }
    }
}
