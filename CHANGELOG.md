# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Terminal mail client for Outlook.com and Microsoft 365, with folder, message
  list and reading panes.
- `tuilook login` signs in with a device code and keeps the refresh token in the
  OS keychain; `tuilook logout` forgets it. Signs in to personal accounts
  (`consumers`) with tuilook's own app registration by default; `--tenant` and
  `--client-id` (or `TUILOOK_TENANT` and `TUILOOK_CLIENT_ID`) override them.
- `tuilook sync` syncs every folder once without the UI, and `tuilook --demo`
  browses a built-in sample mailbox without signing in.
- Offline cache in SQLite: the UI shows cached mail at once while folders sync
  in the background with Graph delta queries.
- HTML bodies are shown as text, with links as numbered footnotes and images,
  table borders and runs of blank lines left out.
- Vim-style navigation: `j`/`k`, `gg`/`G`, Home/End, PageUp/PageDown and
  `Ctrl-F`/`Ctrl-B`/`Ctrl-D`/`Ctrl-U`, paging by what fits on screen.
- Folder tree with subfolders collapsed and expanded by `h`/`l` or `←`/`→`.
  Collapsed folders count the unread mail inside them.
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
- A README on installing, signing in and using tuilook.

[Unreleased]: https://github.com/christianhelle/tuilook/commits/main
