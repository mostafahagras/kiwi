use core::ffi::c_void;
use core_foundation::{
    base::TCFType,
    mach_port::{CFMachPort, CFMachPortInvalidate, CFMachPortRef},
};
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CallbackResult,
};
use foreign_types::ForeignType;
use std::{mem::ManuallyDrop, ptr};

pub type CGEventMask = u64;
pub type CGEventTapProxy = *const c_void;

type CGEventTapCallbackFn<'tap_life> =
    Box<dyn Fn(CGEventTapProxy, CGEventType, &CGEvent) -> CallbackResult + 'tap_life>;

type CGEventTapCallBackInternal = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    etype: u32,
    event: *mut c_void,
    user_info: *const c_void,
) -> *mut c_void;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CGEventType {
    Null = 0,
    KeyDown = 10,
    KeyUp = 11,
    FlagsChanged = 12,
    SystemDefined = 14,
}

impl CGEventType {
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::Null),
            10 => Some(Self::KeyDown),
            11 => Some(Self::KeyUp),
            12 => Some(Self::FlagsChanged),
            14 => Some(Self::SystemDefined),
            _ => None,
        }
    }

    pub const fn as_raw(self) -> u32 {
        self as u32
    }

    const fn to_mask_bit(self) -> CGEventMask {
        let raw = self.as_raw();
        if raw >= 64 {
            0
        } else {
            1u64 << raw
        }
    }
}

unsafe extern "C" fn cg_event_tap_callback_internal(
    proxy: CGEventTapProxy,
    etype: u32,
    event: *mut c_void,
    user_info: *const c_void,
) -> *mut c_void {
    let callback = user_info as *mut CGEventTapCallbackFn;
    let event = ManuallyDrop::new(unsafe { CGEvent::from_ptr(event.cast()) });

    let Some(event_type) = CGEventType::from_raw(etype) else {
        return event.as_ptr().cast();
    };

    use CallbackResult::*;
    let callback_result = unsafe { (*callback)(proxy, event_type, &event) };
    match callback_result {
        Keep => event.as_ptr().cast(),
        Drop => ptr::null_mut(),
        Replace(new_event) => ManuallyDrop::new(new_event).as_ptr().cast(),
    }
}

#[must_use = "CGEventTap is disabled when dropped"]
pub struct CGEventTap<'tap_life> {
    mach_port: CFMachPort,
    _callback: Box<CGEventTapCallbackFn<'tap_life>>,
}

impl CGEventTap<'static> {
    pub fn new<F: Fn(CGEventTapProxy, CGEventType, &CGEvent) -> CallbackResult + Send + 'static>(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        events_of_interest: Vec<CGEventType>,
        callback: F,
    ) -> Result<Self, ()> {
        // SAFETY: callback is 'static so even if this object is forgotten it
        // will be valid to call. F is safe to send across threads.
        unsafe { Self::new_unchecked(tap, place, options, events_of_interest, callback) }
    }
}

impl<'tap_life> CGEventTap<'tap_life> {
    /// Caller is responsible for ensuring that this object is dropped before
    /// `'tap_life` expires. Either state captured by `callback` must be safe to
    /// send across threads, or the tap must only be installed on the current
    /// thread's run loop.
    pub unsafe fn new_unchecked(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        events_of_interest: Vec<CGEventType>,
        callback: impl Fn(CGEventTapProxy, CGEventType, &CGEvent) -> CallbackResult + 'tap_life,
    ) -> Result<Self, ()> {
        let event_mask: CGEventMask = events_of_interest
            .iter()
            .fold(CGEventType::Null as CGEventMask, |mask, etype| {
                mask | etype.to_mask_bit()
            });

        let cb: Box<CGEventTapCallbackFn> = Box::new(Box::new(callback));
        let cbr = Box::into_raw(cb);

        let event_tap_ref = unsafe {
            CGEventTapCreate(
                tap,
                place,
                options,
                event_mask,
                cg_event_tap_callback_internal,
                cbr.cast(),
            )
        };

        if !event_tap_ref.is_null() {
            Ok(Self {
                mach_port: unsafe { CFMachPort::wrap_under_create_rule(event_tap_ref) },
                _callback: unsafe { Box::from_raw(cbr) },
            })
        } else {
            let _ = unsafe { Box::from_raw(cbr) };
            Err(())
        }
    }

    pub fn mach_port(&self) -> &CFMachPort {
        &self.mach_port
    }

    pub fn enable(&self) {
        unsafe { CGEventTapEnable(self.mach_port.as_concrete_TypeRef(), true) }
    }
}

impl Drop for CGEventTap<'_> {
    fn drop(&mut self) {
        unsafe { CFMachPortInvalidate(self.mach_port.as_CFTypeRef() as *mut _) };
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        events_of_interest: CGEventMask,
        callback: CGEventTapCallBackInternal,
        user_info: *const c_void,
    ) -> CFMachPortRef;

    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}
