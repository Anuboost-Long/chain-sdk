//! Page-zoom capability — see /agent-docs/capabilities/page-zoom/CONTRACT.md.
//! The zoom itself is the webview's own page zoom (WKWebView `pageZoom`
//! through Tauri's `Webview::set_zoom`): real reflow at the new size,
//! drawn at full resolution. This module only owns the accepted range.

/// Browsers' own page-zoom range (25% to 500%).
pub const MIN: f64 = 0.25;
pub const MAX: f64 = 5.0;

pub fn check(factor: f64) -> Result<f64, String> {
    if factor.is_finite() && (MIN..=MAX).contains(&factor) {
        Ok(factor)
    } else {
        Err(format!("zoom factor must be between {MIN} and {MAX}, got {factor}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_browser_range_only() {
        for ok in [0.25, 0.5, 1.0, 1.5, 2.0, 5.0] {
            assert_eq!(check(ok), Ok(ok));
        }
        for bad in [0.0, 0.2, 5.1, -1.0, f64::NAN, f64::INFINITY] {
            assert!(check(bad).is_err(), "{bad} was accepted");
        }
    }
}
