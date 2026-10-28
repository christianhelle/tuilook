# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Install scripts: `curl -fsSL https://christianhelle.com/tuilook/install | bash`
  on macOS and Linux, and `irm https://christianhelle.com/tuilook/install.ps1 | iex`
  on Windows. Both install the latest release, or the one `VERSION` / `-Version`
  pins.
- Release builds for Linux and Windows on ARM64.
- An application icon, shown on `otlk.exe` on Windows and atop the README.
- Search: `/` searches every folder of every account as you type, matching
  subject, sender, preview and the text of messages opened before. Results
  show the folder each message is in, and can be read, answered and deleted
  like any folder's.
- Signatures: `S` writes a signature for the account being looked at. New
  messages, replies and forwards from that account start with it, ready to
  edit before sending.
- An Unread section in the folder pane, between Favorites and the accounts. It
  lists each account's folders that have unread mail, leaving out Junk Email
  and Deleted Items, and picking one shows only its unread messages. Mail read
  there stays in the list until the folder is picked again or `F5` is pressed.
- Deleting folders: `d` in the folder pane moves the selected folder and
  everything in it to Deleted Items, and `D` deletes it for good, after asking.
  Outlook's own folders cannot be deleted.
- Date groups in the message list, as in Outlook: Today, Yesterday, This Week,
  Last Week, This Month, Last Month and Older, each under a header with its
  count. `←` folds a group, `→`, `Enter` or a click on its header unfolds it,
  and folded groups stay folded in every folder and across restarts.
- `l`/`→` and `h`/`←` in the reading pane show the next or previous mail,
  zoomed or not, past date headers and folded groups.

### Changed

- Read mail is dimmed and unread subjects are bold, so unread mail stands out
  in the message list.

### Fixed

- Mail older than 90 days can be read. Sync still reaches back 90 days, but
  moving to the end of the message list, or picking a folder that shows
  nothing, fetches the next page of older mail while the folder holds more. A
  folder in the Unread section whose unread mail is all older, which showed an
  empty list, now shows it.
- Accounts folded in the folder pane, in either section, stay folded the next
  time tuilook starts, just as expanded and collapsed folders do.
- A message read, flagged or otherwise changed outside tuilook no longer loses
  its subject, sender and date on the next sync and drops to the bottom of
  its folder. Messages it already blanked are dropped from the cache and
  fetched again on the next sync.

## [0.1.0] - 2026-10-27

### Added

- Terminal mail client for Outlook.com and Microsoft 365, with folder, message
  list and reading panes.
- `otlk login` signs in with a device code and keeps the refresh token in the
  OS keychain; `otlk logout` forgets it. Signs in to personal accounts
  (`consumers`) with tuilook's own app registration by default; `--tenant` and
  `--client-id` (or `TUILOOK_TENANT` and `TUILOOK_CLIENT_ID`) override them.
- `otlk sync` syncs every folder once without the UI, and `otlk --demo`
  browses a built-in sample mailbox without signing in.
- Offline cache in SQLite: the UI shows cached mail at once while folders sync
  in the background with Graph delta queries. Folders deleted on the server
  are dropped from the cache with their messages on the next sync.
- HTML bodies are shown as text, with links as numbered footnotes and images,
  table borders and runs of blank lines left out.
- A message is marked read, in Outlook too, once the reading pane shows it,
  and its folder's unread count goes down.
- Vim-style navigation: `j`/`k`, `gg`/`G`, Home/End, PageUp/PageDown and
  `Ctrl-F`/`Ctrl-B`/`Ctrl-D`/`Ctrl-U`, paging by what fits on screen.
- Folder tree with subfolders collapsed and expanded by `h`/`l` or `←`/`→`.
  Collapsed folders count the unread mail inside them.
- Favorites above the accounts in the folder pane: `*` adds the selected
  folder or takes it out. Favorites with the same name from different accounts
  also show the account, such as `Inbox - me@outlook.com`. Favorites are kept
  between runs.
- `z` zooms the focused pane, and `b` hides or shows the folder pane.
- `?` shows the keys, both while browsing and while writing a message.
- Mark messages with Space, all of them with `Ctrl-A`, and clear the marks
  with Esc.
- `d` moves the marked or selected messages to Deleted Items, and `D` deletes
  them for good, each after asking.
- Write (`c`), reply (`r`), reply to all (`R`) and forward (`f`) in a compose
  screen with To, Cc and Subject fields and a body edited vim style, sent
  with `Ctrl-S`. Replies and forwards quote the original the way Outlook
  does, and characters typed with AltGr work in every field.
- Mail is checked every minute, and again shortly after sending so the sent
  message shows up. `--sync-every <seconds>` (or `TUILOOK_SYNC_EVERY`)
  changes how often, and `0` turns it off. F5 or `Ctrl-R` syncs at once and
  says when mail is up to date.
- Mouse support: click to focus a pane and select a folder or message, and
  scroll with the wheel.
- Picks up where the last session left off: the selected folder and message,
  the focused pane, zoom, hidden folders and expanded folders.
- Status bar messages make way for the key hints again after a few seconds.
- Safeguards against Outlook.com blocking an account for sending in bursts:
  tuilook asks before sending a fourth message within ten minutes, and if
  Outlook.com does block sending, explains how to unblock the account and
  keeps the message open to send later.
- Every request names tuilook in its User-Agent.
- A README on installing, signing in and using tuilook, with step-by-step
  instructions and links to Microsoft's documentation for registering an app
  and finding the client ID and tenant ID for work or school accounts.
- Several accounts side by side: run `otlk login` again to add one. Each
  account heads its own folders in the folder pane, every account syncs in the
  background, and a message is sent from the account whose folder is open,
  shown on a From line while writing. `otlk accounts` lists the accounts,
  `otlk logout <address>` signs out of one, and `otlk sync` syncs them
  all. Each account remembers its tenant and client ID, and a sign-in from
  before this is moved over on first start.
- Sign-ins too long for one keychain entry, as work and school accounts'
  often are on Windows, are split over several entries.
- `otlk --demo` shows three sample accounts (personal, work and
  university) with dozens of messages in nested folders, and starts on the
  first account's Inbox.

### Changed

- The command is now `otlk`, which is quicker to type; the project is still
  tuilook, so `cargo install` installs it under that name. Sign-ins, the mail
  cache and the `TUILOOK_*` environment variables are unchanged.

### Fixed

- Panes no longer smear across each other in terminals such as WezTerm: mail
  is drawn without the invisible characters (zero-width joiners, soft hyphens,
  variation selectors) that newsletters pad their previews with, and tabs
  show as spaces.

[Unreleased]: https://github.com/christianhelle/tuilook/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/christianhelle/tuilook/releases/tag/v0.1.0
