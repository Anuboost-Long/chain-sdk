# PageZoom — Windows Research

**Not run on Windows.** wry's `set_zoom` uses WebView2's
`ICoreWebView2Controller::put_ZoomFactor`, which is also real page zoom
and raises `devicePixelRatio`. Check that, and that the factor survives
a reload there too (WebView2 may reset it on navigation).

- [ ] `set(1.5)`: `innerWidth` drops by 1.5×, `devicePixelRatio` rises.
- [ ] Reload keeps the factor, or `get()` is reset to match.
