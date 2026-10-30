# TUILook

[![crates.io](https://img.shields.io/crates/v/tuilook.svg)](https://crates.io/crates/tuilook)
[![CI](https://github.com/christianhelle/tuilook/actions/workflows/ci.yml/badge.svg)](https://github.com/christianhelle/tuilook/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A terminal mail and calendar client for Outlook.com and Microsoft 365, with
vim-style keys.
It keeps an offline cache, so your mail shows up at once while folders sync in
the background.

![tuilook showing the folder, message and reading panes](images/windows/emails.jpg)

## Install

tuilook installs as the `otlk` command.

### macOS/Linux

```bash
curl -fsSL https://christianhelle.com/tuilook/install | bash
```

This installs the latest release to `~/.local/bin`. To install somewhere else,
or to pin a release:

```bash
curl -fsSL https://christianhelle.com/tuilook/install | INSTALL_DIR="$HOME/bin" bash
curl -fsSL https://christianhelle.com/tuilook/install | VERSION="0.1.2" bash
```

### Windows PowerShell

```powershell
irm https://christianhelle.com/tuilook/install.ps1 | iex
```

This installs the latest release to `%LOCALAPPDATA%\Programs\tuilook`, or to
`~\.local\bin` or `~\bin` if one of those already exists, and adds it to your
user `PATH`. To install somewhere else, or to pin a release:

```powershell
$install = irm https://christianhelle.com/tuilook/install.ps1
& ([scriptblock]::Create($install)) -InstallDir "$env:USERPROFILE\bin"
& ([scriptblock]::Create($install)) -Version "0.1.2"
```

### Cargo

With a recent stable [Rust](https://rustup.rs) toolchain (1.95 or later),
install it from [crates.io](https://crates.io/crates/tuilook):

```bash
cargo install tuilook --locked
```

### Release archives

Download a build from the
[releases page](https://github.com/christianhelle/tuilook/releases), unpack it
and put `otlk` somewhere on your `PATH`. Archives are available for Linux,
macOS and Windows, on x64 and ARM64.

To try changes that aren't released yet, install straight from GitHub:

```bash
cargo install --git https://github.com/christianhelle/tuilook
```

## Try it without signing in

```bash
otlk --demo
```

This opens three built-in sample accounts with nested folders. Nothing is sent
anywhere, so it is a safe place to learn the keys.

## Sign in

```bash
otlk login
```

tuilook prints a code and a link. Open the link in a browser, enter the code
and sign in with your Microsoft account. The sign-in is kept in your OS
keychain, so you only do this once.

Then start tuilook:

```bash
otlk
```

### More than one account

Run `otlk login` again and sign in with another account. tuilook opens
every account at once: each one heads its own folders in the folder pane, and
all of them sync in the background. A message you write is sent from the
account whose folder you are in, and the compose screen says which. To send a
new message from another account, `Shift-Tab` from `To:` up to `From:` and
press `←` `→` (or `Space`) to pick it; an untouched signature changes with it.
A reply or forward is always sent from the account the original is in.

To see which accounts are signed in:

```bash
otlk accounts
```

To sign out of an account and forget its cached mail, give its address (or
enough of the start of it to tell it apart). With only one account, the
address can be left out:

```bash
otlk logout me@outlook.com
```

### Work or school accounts

Microsoft only lets a program read mail if it is registered with Microsoft
Entra ID (formerly Azure Active Directory). The registration gives it a
**client ID**, which Microsoft shows on the sign-in and consent screens and
which organizations use to allow or block apps. It is an ID, not a secret.

tuilook comes with a registration of its own, but it only accepts personal
Microsoft accounts (Outlook.com, Hotmail, Live). That is why
`otlk login` works for those without any options. For a Microsoft 365 work
or school account, use an app registration in Entra ID and tell tuilook its
client ID and which tenant (organization) to sign in to.

If your organization's IT already has a registration for mail clients like
this, ask them for its client ID and tenant ID and skip to
[signing in](#sign-in-with-your-registration). Otherwise, register one
yourself:

#### 1. Register an app

Follow Microsoft's guide
[Register an application in Microsoft Entra ID](https://learn.microsoft.com/en-us/entra/identity-platform/quickstart-register-app).
In short:

1. Sign in to the [Microsoft Entra admin center](https://entra.microsoft.com).
   If you can't register apps in your organization's tenant, you can use the
   free Default Directory that comes with an Azure account.
2. Go to **Entra ID** > **App registrations** > **New registration**.
3. Give it a name, such as `tuilook`.
4. Under **Supported account types**, pick who may sign in with it. This also
   decides what to pass as `--tenant` in step 4:

   | Supported account types | `--tenant` |
   | --- | --- |
   | Single tenant only | your tenant ID |
   | Multiple Entra ID tenants | `organizations`, or a tenant ID |
   | Any Entra ID Tenant + Personal Microsoft accounts | `common`, `organizations` or a tenant ID |
   | Personal accounts only | `consumers` (tuilook's own registration already covers this) |

5. Select **Register**. No redirect URI is needed.

#### 2. Allow device-code sign-in

tuilook signs in with a code you enter in a browser (the
[device authorization grant](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code)),
and has no secret of its own, so the registration must allow public client
flows. See
[Enable public client flow](https://learn.microsoft.com/en-us/entra/identity-platform/scenario-desktop-app-configuration#enable-public-client-flow):

1. In your app registration, select **Authentication**.
2. Under **Advanced settings**, set **Allow public client flows** to **Yes**.
3. Select **Save**.

Without this, signing in fails with `AADSTS7000218`
([what that means](https://learn.microsoft.com/en-us/troubleshoot/entra/entra-id/app-integration/confidential-client-application-authentication-error-aadsts7000218)).

#### 3. Add the mail and calendar permissions

In your app registration, select **API permissions** > **Add a permission** >
**Microsoft Graph** > **Delegated permissions**, then add:

- `User.Read` (usually there already), to learn your email address
- `Mail.ReadWrite`, to read, sync and delete mail
- `Mail.Send`, to send mail
- `MailboxSettings.Read`
- `Calendars.ReadWrite`, to show and change your calendar
- `offline_access`, to stay signed in

Microsoft's guide
[Add permissions to access Microsoft Graph](https://learn.microsoft.com/en-us/entra/identity-platform/quickstart-configure-app-access-web-apis#add-permissions-to-access-microsoft-graph)
walks through this, and the
[Graph permissions reference](https://learn.microsoft.com/en-us/graph/permissions-reference)
says what each one allows.

Many organizations don't let their users approve an app themselves. If
signing in says an administrator must approve tuilook, an admin needs to
[grant admin consent](https://learn.microsoft.com/en-us/entra/identity/enterprise-apps/grant-admin-consent),
for example with **Grant admin consent** on the **API permissions** page.

#### 4. Find the client ID and tenant ID

- **Client ID:** on your app registration's **Overview** page, as
  **Application (client) ID**.
- **Tenant ID:** on the same page, as **Directory (tenant) ID**. That is the
  tenant the app is registered in. For an organization you belong to, see
  [How to find your tenant ID](https://learn.microsoft.com/en-us/entra/fundamentals/how-to-find-tenant)
  (**Entra ID** > **Overview** > **Properties**). The organization's domain,
  such as `contoso.com`, works in its place, and so does `organizations` to
  let Microsoft work out the tenant from the account you sign in with.

#### Sign in with your registration

```bash
otlk --tenant <tenant-id> --client-id <client-id> login
```

Or set `TUILOOK_TENANT` and `TUILOOK_CLIENT_ID` instead of passing the
options. tuilook remembers the tenant and client ID each account signed in
with, so they are only needed for `login`. Accounts in different
organizations can each use their own.

#### If device-code sign-in is blocked

Device-code sign-in can be abused for phishing, so many organizations block
it. Any tenant with Entra ID's
[security defaults](https://learn.microsoft.com/en-us/entra/fundamentals/security-defaults)
turned on blocks it, and tenants created from 1 July 2026 have them on from
the start. Others block it with Conditional Access. If signing in is refused,
tuilook can't sign in to that organization for now; ask its IT whether
device-code sign-in can be allowed for you.

## The screen

Three panes: folders, the messages in the selected folder, and the message you
are reading. `Tab` and `Shift-Tab` move between them, or click a pane with the
mouse. Press `?` at any time to see every key.

![The keys, shown over the panes after pressing ?](images/windows/help.jpg)

tuilook remembers where you were when you quit (folder, message, focused pane,
zoom, expanded folders and favorites) and picks up there next time.

## Reading mail

| Keys | What they do |
| --- | --- |
| `Tab` / `Shift-Tab` | Next pane / previous pane |
| `Enter` | Open the folder or message, or fold or unfold a date group |
| `j` `k` or `↓` `↑` | Move |
| `l` `h` or `→` `←` | Expand a folder or date group, collapse it or go up; in the reading pane, `→` `←` show the next or previous mail |
| `*` | Add the folder to Favorites, or take it out |
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

The message list groups mail by date, as Outlook does: Today, Yesterday,
This Week, Last Week, This Month, Last Month and Older. `←` on a message folds
its group under its header, and `→`, `Enter` or a click on the header unfolds
it again. Folded groups stay folded in every folder, and the next time tuilook
starts.

`p` pins the marked (or selected) messages, and pressing it again unpins them.
Pinned mail sits in a Pinned group above all the dates, however old it is,
with a 📌 after the sender.
Pins are Outlook.com's own, so mail pinned in Outlook.com shows up pinned here,
and the other way round.

`F` or `Insert` flags the marked (or selected) messages for follow-up, and
pressing it again clears their flags. Flagged mail shows a red ⚑ after the
sender, left of any 📌. Flags are Outlook.com's own too, so they show up on both sides.

The folder pane has three parts: Favorites at the top, Unread in the middle and
your accounts' folders below. A favorite works like the folder it stands for.
When the same folder name is a favorite from more than one account, such as
each account's Inbox, the favorite also shows the account, as in
`Inbox - me@outlook.com`.

Unread lists your accounts' folders again, but only those with unread mail,
leaving out Junk Email and Deleted Items. Picking a folder there shows only its
unread messages. Mail you read stays in the list, shown as read, so you can go
back to it, until you pick the folder again or press `F5`; after `F5` only the
message you are reading stays. The folder stays until you leave it.

tuilook keeps the last 90 days of each folder in sync. Older mail is fetched
when you need it: move to the end of the message list, or pick a folder whose
recent mail is all read, and the next 50 older messages are fetched while the
folder holds more than the list shows. In the Unread section only older unread
mail is fetched.

HTML mail is shown as plain text. Links become numbered footnotes at the end of
the message.

### Moving in a message and copying from it

The reading pane works like Vim's normal mode: it has a cursor, shown while
the pane is focused, and the status bar says `NORMAL`. The pane scrolls to keep
the cursor in view, and scrolling with the mouse wheel drags the cursor along.
`v` starts picking text at the cursor, as Vim's visual mode does, and `V` picks
whole lines; `y` then copies what is picked to the clipboard.

Lines are numbered as with Vim's `number` and `relativenumber` both on: the
cursor's line shows its own number and every other line how far it is from the
cursor, so the count to reach it, as in `5j`, can be read off. A line wrapped
over several rows is numbered on its first.

| Keys | What they do |
| --- | --- |
| `h` `j` `k` `l` `w` `b` `e` `0` `^` `$` `gg` `G` | Move the cursor |
| `Ctrl-D` `Ctrl-U` `Ctrl-F` `Ctrl-B` or `PgDn` `PgUp` | Move half a page or a page |
| `v` / `V` | Pick characters / whole lines from the cursor |
| `y` | Copy what is picked to the clipboard |
| `yy` `yw` `y$` and so on | Copy a line, a word, to the end of the line |
| `Esc` | Stop picking |
| `→` `←` | Show the next or previous mail |

What you copy is also kept to put with `p` or `P` in normal mode while writing
a message or a signature, as in Vim. Text deleted or yanked while writing is
kept the same way, from one message to the next.

Keys the cursor has no use for work as they do in the other panes, so `r`
replies and `d` deletes. In the reading pane `b` moves back a word; hide the
folders from another pane.

tuilook copies through the terminal with an OSC 52 escape sequence, so it works
over SSH too. Windows Terminal, WezTerm, kitty, Alacritty, iTerm2 and most
modern terminals support it; tmux needs `set -g set-clipboard on`.

## Searching

| Keys | What they do |
| --- | --- |
| `/` | Search every folder of every account |
| `Enter` | Keep the results and move into them |
| `Esc` | Go back to the folder you were in |

Press `/` and start typing: the message list shows what matches as you type,
newest first, with the folder (and account) each message is in. A message
matches when its subject, sender or preview contains what you typed, ignoring
case, or its text does if you have opened it before. Search looks through the
mail tuilook has cached, so it works offline but does not find mail that has
not synced yet.

You can read, reply to and delete the results as in any folder. `Esc` in the
results (once nothing is marked) or picking a folder ends the search, and `/`
goes back to the query to change it.

## Moving and deleting

| Keys | What they do |
| --- | --- |
| `Space` | Mark or unmark the selected message |
| `Ctrl-A` | Mark every message in the folder |
| `Esc` | Clear the marks |
| `m` | Move the marked (or selected) messages to another folder |
| `p` | Pin the marked (or selected) messages on top, or unpin them |
| `F` `Insert` | Flag the marked (or selected) messages, or clear their flags |
| `d` | Move the marked (or selected) messages to Deleted Items |
| `D` | Delete the marked (or selected) messages for good |

`m` opens a list of the account's folders. Type part of a folder's name to
narrow it, pick one with `↑` `↓`, and press `Enter` to move the mail there, or
`Esc` to leave it where it is. Mail can only be moved within its own account.

Both `d` and `D` ask before doing anything.

![Moving a message to another folder](images/windows/move-mail.jpg)

![Asking before moving 14 marked messages to Deleted Items](images/windows/delete-mails.jpg)

## Folders

In the folder pane:

| Keys | What they do |
| --- | --- |
| `a` | Make a new folder inside the selected one |
| `A` | Make a new top-level folder in the selected folder's account |
| `F2` | Rename the selected folder |
| `d` | Move the selected folder, with its subfolders and mail, to Deleted Items |
| `D` | Delete the selected folder for good |

`a`, `A` and `F2` ask for the name: `Enter` saves it and `Esc` cancels. On an
account's row, `a` makes a top-level folder too. Outlook's own folders, such
as Inbox and Sent Items, cannot be renamed or deleted.

## Writing mail

| Keys | What they do |
| --- | --- |
| `c` | Write a new message |
| `r` | Reply |
| `R` | Reply to all |
| `f` | Forward |
| `S` | Write the account's signature |

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

Counts work too, like `3dd` or `5j`. The body's lines are numbered as in the
reading pane, by how far each is from the cursor.

### Signatures

Press `S` to write a signature for the account you are looking at. Each
account has its own. New messages, replies and forwards from that account
start with it below an empty line, where you type, so you can change or delete
it before sending. It goes in exactly as you write it. Add a `-- ` line above it
yourself if you want one.

The signature is edited vim style like a message body. `Ctrl-S` saves it, and
saving an empty signature turns it off. Signatures are kept in `accounts.json`.
In `--demo` they last until tuilook closes.

### Sending limits on Outlook.com

Outlook.com may block an account that sends many messages in a short time. To
help avoid that, tuilook asks before sending a fourth message within ten
minutes. If Outlook.com does block sending, tuilook explains how to unblock the
account and keeps your message open so you can send it later.

## Calendar

Press `C` to show the calendar of the account you are looking at. The agenda
takes over the screen. It lists each day's appointments, from a week ago to
three months ahead, and opens on today. It shows the cached calendar at once
and syncs it in the background, so it works offline too.

| Keys | What they do |
| --- | --- |
| `j` `k` `↓` `↑` | Move between appointments |
| `g` `G` `t` | Jump to the first, the last, or today's first |
| `Enter` | Show the appointment in full beside the agenda, or hide it |
| `c` | Write a new appointment, on the selected day |
| `e` | Change the selected appointment |
| `d` | Delete it, after asking; anyone invited is told it is cancelled |
| `F5` | Sync the calendar now |
| `Tab` / `Shift-Tab` | Show the next / previous account's calendar |
| `Esc` `q` | Back to the mail |

![The calendar's agenda with an appointment's details beside it](images/windows/calendar.jpg)

A new appointment runs from 9:00 to 10:00 unless you change it. Write the date
as `2026-11-30` and times as `14:30`, in your computer's time zone. Press
`Space` on `All day` to make it last the whole day. Add addresses to `Invite`,
separated by `,` or `;`, to send them an invitation when it is saved. The notes
are edited vim style, like a message body, and `Ctrl-S` saves.

Changing an appointment shows only a preview of its notes, which Outlook may
shorten. The notes are saved only if you edit them, so the full notes stay as
they are otherwise. Changing one occurrence of a repeating appointment changes
only that occurrence. tuilook cannot make an appointment repeat.

If you signed in before tuilook had a calendar, sign in again with
`otlk login` to allow calendar access.

## Syncing

tuilook checks for new mail, and changes to the calendar, every minute, and
again shortly after you send something. Press `F5` or `Ctrl-R` to sync at once.

To change how often it checks, pass the number of seconds, or `0` to check
only at start and when you press `F5`:

```bash
otlk --sync-every 300
```

To sync every account's folders once without opening the UI, for example from
a script:

```bash
otlk sync
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
| `otlk` | Open the mail client with every signed-in account |
| `otlk login` | Sign in with a device code; run again to add an account |
| `otlk logout [address]` | Forget an account's sign-in and cached mail |
| `otlk accounts` | List the signed-in accounts |
| `otlk sync` | Sync every account's folders once, without the UI |

## Where tuilook keeps things

- **Sign-in:** the OS keychain (Windows Credential Manager, macOS Keychain or
  the Secret Service on Linux), one entry per account under its address.
- **Accounts and signatures:** `accounts.json` in tuilook's data folder:
  `~/.local/share/tuilook` on Linux, `~/Library/Application Support/tuilook`
  on macOS, `%APPDATA%\tuilook\data` on Windows.
- **Mail cache:** `<address>/cache.db` in the data folder, one per account.
  Deleting it is safe; tuilook syncs again.
- **Last session:** `session.json` in tuilook's cache folder: `~/.cache/tuilook`
  on Linux, `~/Library/Caches/tuilook` on macOS, `%LOCALAPPDATA%\tuilook\cache`
  on Windows.

## License

tuilook is released under the [MIT License](LICENSE).
