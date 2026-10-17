# Attention — Windows Research

**Not implemented on Windows.** `chain_core::attention` returns
`unavailable` there, while focus and `requestAttention()` already work
through Tauri (`FlashWindowEx` for the taskbar button).

For notifications: WinRT `ToastNotificationManager` needs an AppUserModelID
registered for the app (the installer's Start-menu shortcut), and clicks
come back through `ToastNotification.Activated` while the app runs. The
`windows` crate already in chain-core can call it with the
`UI_Notifications` and `Data_Xml_Dom` features.

Checklist:

- [ ] Toast with title and body from an installed `chain build`.
- [ ] Click → window forward → `onNotificationClick(id)`.
- [ ] `requestAttention()` flashes the taskbar button once.
