//! Where a selector is on the screen, for a gesture that needs a place
//! rather than an element: a long press at a point, the two ends of a drag.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_selector::{Selector, describe_selector};

use crate::{Driver, OCR_RECOGNITION_LEVEL, element_not_found, ocr_locales};

/// `target` as shares of the screen: a point is itself, an `ocrText` the
/// middle of the text it reads, anything else the centre of the element
/// it resolves to.
///
/// # Errors
///
/// `ElementNotFound` when nothing matches — with what is on screen when
/// the tree was asked; a fallback chain is refused, because one place is
/// wanted and a chain names several.
pub async fn locate(d: &dyn Driver, target: &Selector) -> Result<(f64, f64), ExpectationFailure> {
    match target {
        Selector::Point { nx, ny } => Ok((*nx, *ny)),
        Selector::OcrText {
            ocr_text, locales, ..
        } => match d
            .find_text_by_ocr(ocr_text, &ocr_locales(locales), OCR_RECOGNITION_LEVEL)
            .await?
        {
            Some(frame) => Ok((frame.mid_x(), frame.mid_y())),
            None => Err(ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("OCR found no text matching \"{ocr_text}\" on screen"),
                ..Default::default()
            })),
        },
        Selector::Fallback { .. } => Err(ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!(
                "{} is a fallback chain, and this needs one place: name one element or point",
                describe_selector(target)
            ),
            ..Default::default()
        })),
        _ => match d.find_norm_coord(target).await? {
            Some(at) => Ok(at),
            None => Err(element_not_found(&d.tree(None).await?, target)),
        },
    }
}
