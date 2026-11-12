//! A device another live session holds gets its own exit code, so a
//! script running flow after flow can stop at the first refusal instead
//! of failing every flow on the same holder.

use smix_sdk::leased::AdmissionError;

use crate::CliError;

/// What a command exits with when another live session holds the device.
pub(crate) const EXIT_CODE: u8 = 7;

impl From<AdmissionError> for CliError {
    fn from(e: AdmissionError) -> Self {
        match e {
            AdmissionError::InUse { .. } => CliError::DeviceInUse(e.to_string()),
            AdmissionError::HeldByRemains { .. }
            | AdmissionError::NotSettled { .. }
            | AdmissionError::Ledger(_) => CliError::Other(e.to_string()),
        }
    }
}
