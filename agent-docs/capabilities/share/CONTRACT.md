# Share — Semantic Contract

`desktop.share` opens the system share menu (AirDrop, Messages, Mail,
...) for files the app owns. Structural contract:
`capabilities/share/contract.ts`.

Requested by mneme (request 42, for "Share as PDF"): after making a PDF
the student sends it to a friend straight away, instead of saving it
and dragging it into another app.

## `show(files, options?)`

```ts
show(files: SharedFile[], options?: ShareOptions): Promise<ShareResult>
// SharedFile = { reference: string; name?: string }
// ShareResult = { status: "picked"; service: string | null } | { status: "cancelled" }
```

- **Files**: one or more `desktop.files` references. `name` is the file
  name the recipient sees ("Cell biology summary" →
  `Cell biology summary.pdf`): `/`, `\`, `:` and control characters
  become `-`, leading dots go, the reference's extension is added when
  the name has none, and two files with the same name become "name",
  "name 2". Without a name the reference itself is used.
- **`anchor`**: the rectangle the menu points at, in CSS pixels from the
  page's top-left — `element.getBoundingClientRect()` of the button
  that opened it. Without one the menu opens at the centre of the page.
- **`text`**: sent along with the files to services that take text (a
  message body). Some services, AirDrop for one, may send it as a
  separate item.
- **`title`**: the subject for services that have one (Mail).
- **Result**: resolves when the menu closes — `{ status: "picked",
  service }` with the service's name as the menu showed it ("AirDrop",
  "Mail"), or `{ status: "cancelled" }`. Cancelling is never an error.
  "Picked" means a service was chosen; the user can still back out in
  that service's own window.
- One menu at a time per app.

### When the app may delete its file

**As soon as `show()` has been called.** Chain copies each file (a clone
on APFS, so no extra space) into a folder of its own before the menu
opens, under the recipient's name, and hands the services that copy. The
copy is deleted straight away on cancel; after a service was picked it's
kept for a day and deleted by the next `show()` after that. It isn't
deleted when the service says it's done, because some services hold only
the file's location: **Copy** puts it on the clipboard and reports done
at once, and pasting reads the file later. So a copied file pastes for a
day; after that the clipboard points at a file that's gone.

## `availability()`

Never rejects. `available` says whether the share menu exists here; when
it's false, fall back to `files.save`. `anchor`, `text`, `title` and
`serviceName` say which options work. All true on macOS; all false on
Windows and Linux today.

## Errors

| Code | When |
| --- | --- |
| `INVALID_ARGUMENT` | no files, an unknown field, an anchor with non-finite numbers or a negative size |
| `NOT_FOUND` | a reference that isn't an existing file |
| `UNAVAILABLE` | a share menu is already open |
| `UNSUPPORTED` | outside a Chain app, or a platform without an implementation |
| `NATIVE_FAILURE` | the files couldn't be copied, or the menu couldn't open |

## Non-goals

- Sharing straight to one named service, without the menu.
- Receiving shared files (a share extension).
- Sharing text or URLs without a file.
- Telling whether the service actually delivered the files.
