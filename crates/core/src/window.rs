//! The window capability — see agent-docs/capabilities/window/. Options
//! (parsing, defaults, merging, what each platform does) are portable;
//! applying them to an NSWindow and measuring its chrome is `macos`.
//! Tauri's own window calls (theme, background colour, decorations,
//! dragging, full screen) are made by the app template's window.rs.

use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct InvalidOptions(pub String);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TitleBarStyle {
    #[default]
    Standard,
    Transparent,
    Overlay,
    Hidden,
}

impl TitleBarStyle {
    /// Whether the page starts at the window's top, under where the bar would be.
    pub fn page_under_bar(self) -> bool {
        matches!(self, Self::Overlay | Self::Hidden)
    }

    pub fn supported(self) -> bool {
        cfg!(target_os = "macos") || matches!(self, Self::Standard | Self::Hidden)
    }
}

/// The bar heights macOS lays out itself, window buttons included.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TitleBarSize {
    #[default]
    Standard,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ButtonsPosition {
    pub left: f64,
    pub center_y: f64,
}

/// `null` → `Some(None)` (back to the default), absent → `None` (unchanged).
fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// `WindowOptions` in contract.ts: what package.json and setOptions() carry.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WindowOptions {
    #[serde(default, deserialize_with = "nullable")]
    pub title_bar_style: Option<Option<TitleBarStyle>>,
    #[serde(default, deserialize_with = "nullable")]
    pub title_bar_size: Option<Option<TitleBarSize>>,
    #[serde(default, deserialize_with = "nullable")]
    pub title_visible: Option<Option<bool>>,
    pub window_buttons: Option<ButtonsOptions>,
    #[serde(default, deserialize_with = "nullable")]
    pub appearance: Option<Option<Appearance>>,
    #[serde(default, deserialize_with = "nullable")]
    pub background_color: Option<Option<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ButtonsOptions {
    #[serde(default, deserialize_with = "nullable")]
    pub close: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    pub minimize: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    pub zoom: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable")]
    pub position: Option<Option<ButtonsPosition>>,
}

pub type Rgba = [u8; 4];

/// A window's chrome as asked for, defaults included.
#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    pub style: TitleBarStyle,
    pub size: TitleBarSize,
    /// None = the style's default.
    pub title_visible: Option<bool>,
    pub close: bool,
    pub minimize: bool,
    pub zoom: bool,
    pub position: Option<ButtonsPosition>,
    pub appearance: Appearance,
    pub background_color: Option<Rgba>,
}

impl Default for Chrome {
    fn default() -> Self {
        Self {
            style: TitleBarStyle::Standard,
            size: TitleBarSize::Standard,
            title_visible: None,
            close: true,
            minimize: true,
            zoom: true,
            position: None,
            appearance: Appearance::System,
            background_color: None,
        }
    }
}

/// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
pub fn parse_color(text: &str) -> Result<Rgba, InvalidOptions> {
    let invalid = || InvalidOptions(format!("backgroundColor must be #rgb, #rgba, #rrggbb or #rrggbbaa, got {text:?}"));
    let hex = text.strip_prefix('#').filter(|h| h.chars().all(|c| c.is_ascii_hexdigit())).ok_or_else(invalid)?;
    let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).expect("checked hex digit");
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("checked hex digits");
    match hex.len() {
        3 | 4 => {
            let alpha = if hex.len() == 4 { digit(3) * 17 } else { 255 };
            Ok([digit(0) * 17, digit(1) * 17, digit(2) * 17, alpha])
        }
        6 | 8 => {
            let alpha = if hex.len() == 8 { pair(6) } else { 255 };
            Ok([pair(0), pair(2), pair(4), alpha])
        }
        _ => Err(invalid()),
    }
}

pub fn format_color([r, g, b, a]: Rgba) -> String {
    format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
}

fn check_position(position: &ButtonsPosition) -> Result<(), InvalidOptions> {
    for (name, value) in [("left", position.left), ("centerY", position.center_y)] {
        if !value.is_finite() || value < 0.0 {
            return Err(InvalidOptions(format!(
                "windowButtons.position.{name} must be a non-negative number of pixels, got {value}"
            )));
        }
    }
    Ok(())
}

impl Chrome {
    /// The `"chain": { "window": ... }` part of the app's package.json,
    /// over the defaults.
    pub fn from_package_json(text: &str) -> Result<Self, InvalidOptions> {
        let in_package = |e: InvalidOptions| InvalidOptions(format!("package.json \"chain.window\": {}", e.0));
        let package: serde_json::Value =
            serde_json::from_str(text).map_err(|e| in_package(InvalidOptions(e.to_string())))?;
        let mut chrome = Self::default();
        match package.get("chain").and_then(|chain| chain.get("window")) {
            None | Some(serde_json::Value::Null) => {}
            Some(window) => chrome.merge(window.clone()).map_err(in_package)?,
        }
        Ok(chrome)
    }

    /// Applies a `WindowOptions` JSON value — all of it, or none of it
    /// when anything in it is invalid.
    pub fn merge(&mut self, options: serde_json::Value) -> Result<(), InvalidOptions> {
        let options: WindowOptions = serde_json::from_value(options).map_err(|e| InvalidOptions(e.to_string()))?;
        let background_color = match &options.background_color {
            Some(Some(text)) => Some(Some(parse_color(text)?)),
            Some(None) => Some(None),
            None => None,
        };
        let buttons = options.window_buttons.unwrap_or_default();
        if let Some(Some(position)) = &buttons.position {
            check_position(position)?;
        }

        let defaults = Self::default();
        if let Some(style) = options.title_bar_style {
            self.style = style.unwrap_or(defaults.style);
        }
        if let Some(size) = options.title_bar_size {
            self.size = size.unwrap_or(defaults.size);
        }
        if let Some(visible) = options.title_visible {
            self.title_visible = visible;
        }
        if let Some(close) = buttons.close {
            self.close = close.unwrap_or(defaults.close);
        }
        if let Some(minimize) = buttons.minimize {
            self.minimize = minimize.unwrap_or(defaults.minimize);
        }
        if let Some(zoom) = buttons.zoom {
            self.zoom = zoom.unwrap_or(defaults.zoom);
        }
        if let Some(position) = buttons.position {
            self.position = position;
        }
        if let Some(appearance) = options.appearance {
            self.appearance = appearance.unwrap_or(defaults.appearance);
        }
        if let Some(color) = background_color {
            self.background_color = color;
        }
        Ok(())
    }

    /// The style this platform actually shows.
    pub fn effective_style(&self) -> TitleBarStyle {
        if self.style.supported() {
            self.style
        } else {
            TitleBarStyle::Standard
        }
    }

    /// The size this platform actually shows: only macOS has the larger
    /// bars, and `hidden` has no bar.
    pub fn effective_size(&self) -> TitleBarSize {
        if cfg!(target_os = "macos") && self.effective_style() != TitleBarStyle::Hidden {
            self.size
        } else {
            TitleBarSize::Standard
        }
    }

    pub fn title_shown(&self) -> bool {
        if !cfg!(target_os = "macos") {
            return true;
        }
        match self.effective_style() {
            TitleBarStyle::Hidden => false,
            TitleBarStyle::Overlay => self.title_visible.unwrap_or(false),
            TitleBarStyle::Standard | TitleBarStyle::Transparent => self.title_visible.unwrap_or(true),
        }
    }

    pub fn resolved(&self) -> ResolvedOptions {
        let mac = cfg!(target_os = "macos");
        let shown = |button: bool| !mac || (button && self.effective_style() != TitleBarStyle::Hidden);
        ResolvedOptions {
            title_bar_style: self.effective_style(),
            title_bar_size: self.effective_size(),
            title_visible: self.title_shown(),
            window_buttons: ResolvedButtons {
                close: shown(self.close),
                minimize: shown(self.minimize),
                zoom: shown(self.zoom),
                position: self.position.filter(|_| mac),
            },
            appearance: self.appearance,
            background_color: self.background_color.map(format_color),
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedOptions {
    pub title_bar_style: TitleBarStyle,
    pub title_bar_size: TitleBarSize,
    pub title_visible: bool,
    pub window_buttons: ResolvedButtons,
    pub appearance: Appearance,
    pub background_color: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedButtons {
    pub close: bool,
    pub minimize: bool,
    pub zoom: bool,
    pub position: Option<ButtonsPosition>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// `TitleBarInsets` in contract.ts. All zero where no chrome is drawn over the page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Insets {
    pub height: f64,
    pub left: f64,
    pub right: f64,
    pub window_buttons: Option<Rect>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleBarStyles {
    standard: bool,
    transparent: bool,
    overlay: bool,
    hidden: bool,
}

/// `WindowAvailability` in contract.ts, for a Chain window on this platform.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    available: bool,
    title_bar_styles: TitleBarStyles,
    title_bar_size: bool,
    title_visible: bool,
    window_buttons_position: bool,
    window_buttons_visibility: bool,
    appearance: bool,
    background_color: bool,
    drag_regions: bool,
    start_drag: bool,
    title_bar_insets: bool,
    full_screen: bool,
}

pub fn availability() -> Availability {
    let mac = cfg!(target_os = "macos");
    Availability {
        available: true,
        title_bar_styles: TitleBarStyles {
            standard: true,
            transparent: TitleBarStyle::Transparent.supported(),
            overlay: TitleBarStyle::Overlay.supported(),
            hidden: true,
        },
        title_bar_size: mac,
        title_visible: mac,
        window_buttons_position: mac,
        window_buttons_visibility: mac,
        appearance: true,
        background_color: true,
        drag_regions: true,
        start_drag: true,
        title_bar_insets: mac,
        full_screen: true,
    }
}

#[cfg(target_os = "macos")]
pub use macos::{apply, insets, page_draws_background, set_drag_regions, title_bar_double_click};

#[cfg(target_os = "macos")]
mod macos {
    use super::{Chrome, Insets, Rect, TitleBarSize, TitleBarStyle};
    use objc2::rc::Retained;
    use objc2_app_kit::{NSButton, NSWindow, NSWindowButton, NSWindowStyleMask, NSWindowTitleVisibility};
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{ns_string, NSNumber, NSPoint, NSRect};

    const BUTTONS: [NSWindowButton; 3] =
        [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton];

    fn window(ns_window: *mut std::ffi::c_void) -> Option<Retained<NSWindow>> {
        // SAFETY: Tauri's Window::ns_window(), alive while the window is.
        unsafe { Retained::retain(ns_window.cast::<NSWindow>()) }
    }

    fn buttons(window: &NSWindow) -> Option<[Retained<NSButton>; 3]> {
        let [close, minimize, zoom] = BUTTONS.map(|b| window.standardWindowButton(b));
        Some([close?, minimize?, zoom?])
    }

    fn full_screen(window: &NSWindow) -> bool {
        window.styleMask().contains(NSWindowStyleMask::FullScreen)
    }

    /// The system title bar's height — `contentLayoutRect` leaves it out
    /// even when the page runs under it.
    fn bar_height(window: &NSWindow) -> f64 {
        let frame = window.frame();
        let content = window.contentLayoutRect();
        (frame.size.height - (content.origin.y + content.size.height)).max(0.0)
    }

    /// Sets the window's style, title and buttons to `chrome`. Main thread.
    pub fn apply(ns_window: *mut std::ffi::c_void, chrome: &Chrome) {
        let Some(window) = window(ns_window) else { return };
        let style = chrome.effective_style();
        let mut mask = window.styleMask();
        mask.set(NSWindowStyleMask::FullSizeContentView, style.page_under_bar());
        if mask != window.styleMask() {
            window.setStyleMask(mask);
        }
        window.setTitlebarAppearsTransparent(style != TitleBarStyle::Standard);
        window.setTitleVisibility(if chrome.title_shown() {
            NSWindowTitleVisibility::Visible
        } else {
            NSWindowTitleVisibility::Hidden
        });
        if let Some(buttons) = buttons(&window) {
            let hidden = style == TitleBarStyle::Hidden;
            for (button, shown) in buttons.iter().zip([chrome.close, chrome.minimize, chrome.zoom]) {
                button.setHidden(hidden || !shown);
            }
        }
        let size = match chrome.effective_size() {
            TitleBarSize::Standard => 0,
            TitleBarSize::Medium => 1,
            TitleBarSize::Large => 2,
        };
        let position = chrome.position.filter(|_| style != TitleBarStyle::Hidden);
        let (left, center_y) = position.map_or((0.0, 0.0), |p| (p.left, p.center_y));
        // SAFETY: as window() above; both run on the main thread.
        unsafe {
            chain_window_set_title_bar_size(ns_window, size);
            chain_window_place_buttons(ns_window, position.is_some(), left, center_y);
        }
    }

    /// The chrome over the page, in points from its top-left. Main thread.
    pub fn insets(ns_window: *mut std::ffi::c_void, chrome: &Chrome) -> Insets {
        let Some(window) = window(ns_window) else { return Insets::default() };
        let style = chrome.effective_style();
        if full_screen(&window) || !style.page_under_bar() {
            return Insets::default();
        }
        let window_height = window.frame().size.height;
        let visible: Vec<NSRect> = buttons(&window)
            .into_iter()
            .flatten()
            .filter(|button| !button.isHidden())
            .map(|button| button.convertRect_toView(button.bounds(), None))
            .collect();
        let window_buttons = visible.into_iter().reduce(|a, b| a.union(b)).map(|r| Rect {
            x: r.origin.x,
            y: window_height - (r.origin.y + r.size.height),
            width: r.size.width,
            height: r.size.height,
        });
        Insets {
            height: if style == TitleBarStyle::Overlay { bar_height(&window) } else { 0.0 },
            left: window_buttons.map_or(0.0, |r| r.x + r.width),
            right: 0.0,
            window_buttons,
        }
    }

    /// Whether the page's WKWebView paints its own default background
    /// (white, or dark grey in dark mode) behind the page. Off while the
    /// app sets a window background colour, so that colour shows before
    /// the page paints. Tauri doesn't do this on macOS; there's no public
    /// API for it — it's the KVC key wry sets for transparent windows.
    /// Main thread, from `with_webview`.
    pub fn page_draws_background(webview: *mut std::ffi::c_void, draws: bool) {
        // SAFETY: Tauri's PlatformWebview::inner(), the WKWebView, valid
        // for the with_webview callback this runs in.
        let Some(webview) = (unsafe { Retained::retain(webview.cast::<AnyObject>()) }) else { return };
        let value = NSNumber::new_bool(draws);
        let _: () = unsafe { msg_send![&*webview, setValue: &*value, forKey: ns_string!("drawsBackground")] };
    }

    extern "C" {
        fn chain_window_set_drag_regions(
            webview: *mut std::ffi::c_void,
            regions: *const f64,
            region_count: usize,
            holes: *const f64,
            hole_count: usize,
        );
        fn chain_window_title_bar_double_click(ns_window: *mut std::ffi::c_void);
        fn chain_window_set_title_bar_size(ns_window: *mut std::ffi::c_void, size: isize);
        fn chain_window_place_buttons(ns_window: *mut std::ffi::c_void, keep: bool, left: f64, center_y: f64);
    }

    fn flat(rects: &[Rect]) -> Vec<f64> {
        rects.iter().flat_map(|r| [r.x, r.y, r.width, r.height]).collect()
    }

    /// Where a press on the page drags the window: inside `regions`, outside
    /// `holes`, in CSS pixels from the page's top-left. A view over the
    /// page takes those presses (swift/ChainWindow.swift); the rest still
    /// reach the page. Main thread, from `with_webview`.
    pub fn set_drag_regions(webview: *mut std::ffi::c_void, regions: &[Rect], holes: &[Rect]) {
        if webview.is_null() {
            return;
        }
        let (regions, holes) = (flat(regions), flat(holes));
        // SAFETY: Tauri's PlatformWebview::inner(), the WKWebView, valid for
        // the with_webview callback this runs in; each slice is 4 f64s per rect.
        unsafe { chain_window_set_drag_regions(webview, regions.as_ptr(), regions.len() / 4, holes.as_ptr(), holes.len() / 4) };
    }

    /// What double-clicking the title bar does, per System Settings. Main thread.
    pub fn title_bar_double_click(ns_window: *mut std::ffi::c_void) {
        if !ns_window.is_null() {
            // SAFETY: Tauri's Window::ns_window(), alive while the window is.
            unsafe { chain_window_title_bar_double_click(ns_window) };
        }
    }

    trait Union {
        fn union(self, other: Self) -> Self;
    }

    impl Union for NSRect {
        fn union(self, other: Self) -> Self {
            let left = self.origin.x.min(other.origin.x);
            let bottom = self.origin.y.min(other.origin.y);
            let right = (self.origin.x + self.size.width).max(other.origin.x + other.size.width);
            let top = (self.origin.y + self.size.height).max(other.origin.y + other.size.height);
            NSRect::new(NSPoint::new(left, bottom), objc2_foundation::NSSize::new(right - left, top - bottom))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn no_window_options_is_todays_window() {
        let chrome = Chrome::from_package_json(r#"{ "name": "app", "chain": { "gpl": false } }"#).unwrap();
        assert_eq!(chrome, Chrome::default());
        assert_eq!(Chrome::from_package_json(r#"{ "name": "app" }"#).unwrap(), Chrome::default());
    }

    #[test]
    fn reads_options_from_package_json() {
        let chrome = Chrome::from_package_json(
            r##"{ "chain": { "window": {
                "titleBarStyle": "overlay",
                "windowButtons": { "zoom": false, "position": { "left": 16, "centerY": 22 } },
                "appearance": "dark",
                "backgroundColor": "#101418"
            } } }"##,
        )
        .unwrap();
        assert_eq!(chrome.style, TitleBarStyle::Overlay);
        assert!(!chrome.zoom && chrome.close);
        assert_eq!(chrome.position, Some(ButtonsPosition { left: 16.0, center_y: 22.0 }));
        assert_eq!(chrome.appearance, Appearance::Dark);
        assert_eq!(chrome.background_color, Some([0x10, 0x14, 0x18, 0xff]));
    }

    #[test]
    fn invalid_package_json_options_name_the_key() {
        let error = Chrome::from_package_json(r#"{ "chain": { "window": { "titleBarStyle": "glass" } } }"#).unwrap_err();
        assert!(error.0.starts_with("package.json \"chain.window\": ") && error.0.contains("glass"), "{}", error.0);
        let error = Chrome::from_package_json(r#"{ "chain": { "window": { "titlebar": "overlay" } } }"#).unwrap_err();
        assert!(error.0.contains("titlebar"), "{}", error.0);
    }

    #[test]
    fn merging_changes_only_what_is_given_and_null_resets() {
        let mut chrome = Chrome::default();
        chrome
            .merge(json!({ "titleBarStyle": "overlay", "titleVisible": true, "backgroundColor": "#fff",
                           "windowButtons": { "minimize": false, "position": { "left": 12, "centerY": 20 } } }))
            .unwrap();
        chrome.merge(json!({ "windowButtons": { "zoom": false } })).unwrap();
        assert!(!chrome.minimize && !chrome.zoom && chrome.position.is_some());
        chrome.merge(json!({ "titleVisible": null, "backgroundColor": null, "windowButtons": { "position": null } })).unwrap();
        assert_eq!(chrome.title_visible, None);
        assert_eq!(chrome.background_color, None);
        assert_eq!(chrome.position, None);
        assert_eq!(chrome.style, TitleBarStyle::Overlay);
        chrome.merge(json!({ "titleBarStyle": null, "windowButtons": { "minimize": null, "zoom": null } })).unwrap();
        assert_eq!(chrome, Chrome::default());
    }

    #[test]
    fn an_invalid_merge_changes_nothing() {
        let mut chrome = Chrome::default();
        let bad = [
            json!({ "titleBarStyle": "overlay", "backgroundColor": "red" }),
            json!({ "titleBarStyle": "overlay", "windowButtons": { "position": { "left": -1, "centerY": 0 } } }),
            json!({ "titleBarStyle": "overlay", "windowButtons": { "position": { "left": 1 } } }),
            json!({ "titleBarStyle": "glass" }),
            json!({ "extra": true }),
        ];
        for options in bad {
            assert!(chrome.merge(options.clone()).is_err(), "{options}");
            assert_eq!(chrome, Chrome::default(), "{options}");
        }
    }

    #[test]
    fn parses_css_hex_colors() {
        assert_eq!(parse_color("#abc").unwrap(), [0xaa, 0xbb, 0xcc, 0xff]);
        assert_eq!(parse_color("#abc8").unwrap(), [0xaa, 0xbb, 0xcc, 0x88]);
        assert_eq!(parse_color("#A1B2C3").unwrap(), [0xa1, 0xb2, 0xc3, 0xff]);
        assert_eq!(parse_color("#a1b2c380").unwrap(), [0xa1, 0xb2, 0xc3, 0x80]);
        for bad in ["abc", "#ab", "#abcde", "#ggg", "#", "#a1b2c3d4e5"] {
            assert!(parse_color(bad).is_err(), "{bad}");
        }
        assert_eq!(format_color([0x0a, 0xff, 0, 0x80]), "#0aff0080");
    }

    #[test]
    fn title_text_defaults_follow_the_style() {
        let mut chrome = Chrome::default();
        assert!(chrome.title_shown());
        chrome.style = TitleBarStyle::Overlay;
        assert_eq!(chrome.title_shown(), !cfg!(target_os = "macos"));
        chrome.title_visible = Some(true);
        assert!(chrome.title_shown());
        chrome.style = TitleBarStyle::Hidden;
        assert_eq!(chrome.title_shown(), !cfg!(target_os = "macos"));
    }

    #[test]
    fn resolved_options_show_what_this_platform_does() {
        let mut chrome = Chrome::default();
        chrome.merge(json!({ "titleBarStyle": "overlay", "windowButtons": { "zoom": false } })).unwrap();
        let resolved = chrome.resolved();
        if cfg!(target_os = "macos") {
            assert_eq!(resolved.title_bar_style, TitleBarStyle::Overlay);
            assert!(!resolved.window_buttons.zoom && !resolved.title_visible);
        } else {
            assert_eq!(resolved.title_bar_style, TitleBarStyle::Standard);
            assert!(resolved.window_buttons.zoom && resolved.title_visible);
        }
        chrome.style = TitleBarStyle::Hidden;
        assert_eq!(chrome.resolved().title_bar_style, TitleBarStyle::Hidden);
    }
}
