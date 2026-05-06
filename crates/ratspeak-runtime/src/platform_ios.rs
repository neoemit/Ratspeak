/// Probe CoreBluetooth authorization without triggering the system prompt.
/// Returns `CBManagerAuthorization` as a string (iOS 13.1+).
pub fn bluetooth_authorization() -> &'static str {
    use objc2::runtime::AnyClass;
    use objc2::{class, msg_send};

    // SAFETY: `+[CBManager authorization]` is a documented class method returning
    // CBManagerAuthorization (NSInteger) synchronously; does not instantiate
    // a central manager and does not prompt the user.
    #[link(name = "CoreBluetooth", kind = "framework")]
    unsafe extern "C" {}

    let cls: &AnyClass = class!(CBManager);
    let raw: i64 = unsafe { msg_send![cls, authorization] };
    match raw {
        0 => "not_determined",
        1 => "restricted",
        2 => "denied",
        3 => "authorized",
        _ => "unknown",
    }
}
