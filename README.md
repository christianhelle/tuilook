# tuilook

A terminal mail client for Outlook.com and Microsoft 365, with vim-style keys.
It keeps an offline cache, so your mail shows up at once while folders sync in
the background.

## Install

You need a recent stable [Rust](https://rustup.rs) toolchain.

```bash
cargo install --git https://github.com/christianhelle/tuilook
```

## Try it without signing in

```bash
tuilook --demo
```

This opens three built-in sample accounts with nested folders. Nothing is sent
anywhere, so it is a safe place to learn the keys.

## Sign in

```bash
tuilook login
```

tuilook prints a code and a link. Open the link in a browser, enter the code
and sign in with your Microsoft account. The sign-in is kept in your OS
keychain, so you only do this once.

Then start tuilook:

```bash
tuilook
```

### More than one account

Run `tuilook login` again and sign in with another account. tuilook opens
every account at once: each one heads its own folders in the folder pane, and
all of them sync in the background. A message you write is sent from the
account whose folder you are in, and the compose screen says which.

To see which accounts are signed in:

```bash
tuilook accounts
```

To sign out of an account and forget its cached mail, give its address (or
enough of the start of it to tell it apart). With only one account, the
address can be left out:

```bash
tuilook logout me@outlook.com
```

### Work or school accounts

By default tuilook signs in to personal Microsoft accounts (Outlook.com,
Hotmail, Live). tuilook's own app registration only accepts those, so for a
Microsoft 365 work or school account, register a public client app in Entra ID
with the delegated Graph permissions `User.Read`, `Mail.ReadWrite`,
`Mail.Send`, `MailboxSettings.Read` and `offline_access`, and pass its ID and
your tenant:

```bash
tuilook --tenant organizations --client-id <your-app-id> login
```

tuilook remembers the tenant and app registration each account signed in
with, so the options are only needed for `login`.

## The screen

```
┌ Folders ─────┐┌ Messages ───────────────────┐┌ Message ─────────────┐
│ Inbox     3  ││ ● Alice   Lunch on Friday?  ││ From: Alice          │
│ Drafts       ││   Bob     Re: Quarterly...  ││ Subject: Lunch ...   │
│ Sent Items   ││   Carol   Invoice #1042     ││                      │
│ ▸ Projects 2 ││                             ││ Are you free ...     │
└──────────────┘└─────────────────────────────┘└──────────────────────┘
 ? help · Tab switch pane · Enter open · c write · r reply · d delete · q quit
```

Three panes: folders, the messages in the selected folder, and the message you
are reading. `Tab` and `Shift-Tab` move between them, or click a pane with the
mouse. Press `?` at any time to see every key.

tuilook remembers where you were when you quit (folder, message, focused pane,
zoom and expanded folders) and picks up there next time.

## Reading mail

| Keys | What they do |
| --- | --- |
| `Tab` / `Shift-Tab` | Next pane / previous pane |
| `Enter` | Open the folder or message |
| `j` `k` or `↓` `↑` | Move, or scroll the message |
| `l` `h` or `→` `←` | Expand a folder, collapse it or go up |
| `PgDn` `PgUp` or `Ctrl-F` `Ctrl-B` | Move or scroll a page |
| `Ctrl-D` `Ctrl-U` | Move or scroll half a page |
| `gg` `G` or `Home` `End` | Jump to the first or last |
| `z` | Zoom the focused pane, or show every pane again |
| `b` | Hide or show the folders |
| `F5` or `Ctrl-R` | Sync every folder now |
| `?` | Show the keys |
| `q` or `Ctrl-C` | Quit |

Subfolders start collapsed. A collapsed folder shows the unread count of
everything inside it.

HTML mail is shown as plain text. Links become numbered footnotes at the end of
the message.

## Deleting

| Keys | What they do |
| --- | --- |
| `Space` | Mark or unmark the selected message |
| `Ctrl-A` | Mark every message in the folder |
| `Esc` | Clear the marks |
| `d` | Move the marked (or selected) messages to Deleted Items |
| `D` | Delete the marked (or selected) messages for good |

Both `d` and `D` ask before doing anything.

## Writing mail

| Keys | What they do |
| --- | --- |
| `c` | Write a new message |
| `r` | Reply |
| `R` | Reply to all |
| `f` | Forward |

The compose screen has To, Cc and Subject fields and a body. Separate several
addresses with `,` or `;`. Replies and forwards quote the original the way
Outlook does.

| Keys | What they do |
| --- | --- |
| `Tab` / `Shift-Tab` | Next field / previous field |
| `Enter` `↓` `↑` | Move between To, Cc and Subject |
| `Ctrl-S` | Send |
| `Esc` (in a field or in normal mode) | Close, asking first if anything was written |
| `?` (in normal mode) | Show the keys |

The body is edited vim style. It starts in normal mode; press `i` to type.

| Keys | What they do |
| --- | --- |
| `i` `a` `I` `A` `o` `O` | Start typing (insert mode) |
| `Esc` | Back to normal mode |
| `h` `j` `k` `l` `w` `b` `e` `0` `^` `$` `gg` `G` | Move |
| `x` `X` `D` `C` `J` `r` | Delete, change, join, replace a character |
| `d` `c` `y` + a motion, e.g. `dd` `cw` `diw` `yy` | Delete, change or copy |
| `p` `P` | Paste after / before |
| `u` `Ctrl-R` | Undo / redo |
| `v` `V` | Select characters / lines |

Counts work too, like `3dd` or `5j`.

### Sending limits on Outlook.com

Outlook.com may block an account that sends many messages in a short time. To
help avoid that, tuilook asks before sending a fourth message within ten
minutes. If Outlook.com does block sending, tuilook explains how to unblock the
account and keeps your message open so you can send it later.

## Syncing

tuilook checks for new mail every minute, and again shortly after you send
something. Press `F5` or `Ctrl-R` to sync at once.

To change how often it checks, pass the number of seconds, or `0` to check
only at start and when you press `F5`:

```bash
tuilook --sync-every 300
```

To sync every account's folders once without opening the UI, for example from
a script:

```bash
tuilook sync
```

## Options

| Option | Environment variable | Default | Meaning |
| --- | --- | --- | --- |
| `--demo` | | | Browse a sample mailbox instead of signing in |
| `--sync-every <seconds>` | `TUILOOK_SYNC_EVERY` | `60` | Seconds between checks for new mail; `0` turns it off |
| `--tenant <tenant>` | `TUILOOK_TENANT` | `consumers` | `consumers`, `organizations`, `common` or a tenant ID to sign in to |
| `--client-id <id>` | `TUILOOK_CLIENT_ID` | tuilook's own | Entra ID app (client) ID to sign in with |

| Command | What it does |
| --- | --- |
| `tuilook` | Open the mail client with every signed-in account |
| `tuilook login` | Sign in with a device code; run again to add an account |
| `tuilook logout [address]` | Forget an account's sign-in and cached mail |
| `tuilook accounts` | List the signed-in accounts |
| `tuilook sync` | Sync every account's folders once, without the UI |

## Where tuilook keeps things

- **Sign-in:** the OS keychain (Windows Credential Manager, macOS Keychain or
  the Secret Service on Linux), one entry per account under its address.
- **Accounts:** `accounts.json` in tuilook's data folder:
  `~/.local/share/tuilook` on Linux, `~/Library/Application Support/tuilook`
  on macOS, `%APPDATA%\tuilook\data` on Windows.
- **Mail cache:** `<address>/cache.db` in the data folder, one per account.
  Deleting it is safe; tuilook syncs again.
- **Last session:** `session.json` in tuilook's cache folder: `~/.cache/tuilook`
  on Linux, `~/Library/Caches/tuilook` on macOS, `%LOCALAPPDATA%\tuilook\cache`
  on Windows.
