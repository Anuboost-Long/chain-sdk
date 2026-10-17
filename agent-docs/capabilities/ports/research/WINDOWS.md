# Ports — Windows Research

**Not compiled or run on Windows.**

- `std` does **not** set `SO_REUSEADDR` on Windows (it would mean
  something else there, closer to port hijacking), and Windows' rule
  differs: a wildcard bind conflicts with a specific-address listener and
  vice versa unless `SO_EXCLUSIVEADDRUSE` is involved. Probing four
  addresses is still correct, but the table in `MACOS.md` must be redone
  on Windows.
- A bind refused by a firewall or an excluded port range
  (`netsh int ipv4 show excludedportrange`, common with Hyper-V/WSL) comes
  back as `WSAEACCES` → `PermissionDenied` → "not free", which is what a
  dev server would hit too.
- `EAFNOSUPPORT` is `WSAEAFNOSUPPORT` (10047) for a machine without IPv6.

Checklist:

- [ ] Redo the four-by-four table with Node servers.
- [ ] A port in an excluded range reads as not free.
- [ ] 100 sequential probes stay fast.
