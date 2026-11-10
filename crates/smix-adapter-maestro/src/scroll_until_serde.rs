//! `scrollUntilVisible`'s reach and timeout, in the flow's own field names.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smix_driver::ScrollUntil;
use smix_host_coord_resolver::Reach;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    visibility_percentage: f64,
    center_element: bool,
    timeout_ms: u64,
}

pub fn serialize<S: Serializer>(u: &ScrollUntil, s: S) -> Result<S::Ok, S::Error> {
    Wire {
        visibility_percentage: u.reach.visibility * 100.0,
        center_element: u.reach.center_element,
        timeout_ms: u64::try_from(u.timeout.as_millis()).unwrap_or(u64::MAX),
    }
    .serialize(s)
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ScrollUntil, D::Error> {
    let w = Wire::deserialize(d)?;
    Ok(ScrollUntil {
        reach: Reach {
            visibility: w.visibility_percentage / 100.0,
            center_element: w.center_element,
        },
        timeout: Duration::from_millis(w.timeout_ms),
    })
}
