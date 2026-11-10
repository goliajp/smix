use smix_sdk::ExpectationFailure;

/// A step this app driver cannot carry out, refused by name rather than
/// passed for something it never did.
pub(super) fn refused(message: impl Into<String>) -> ExpectationFailure {
    ExpectationFailure::new(smix_sdk::FailureInit {
        code: Some(smix_sdk::FailureCode::DriverError),
        message: message.into(),
        ..Default::default()
    })
}
