# Glossary

Words the code and its docs use for tuilook's own ideas.

**Dialog**: a box over the panes that takes every key until it closes:
the help, a blocked send, a confirmation, the name prompt or the move picker.
A dialog owns its state and answers each key with a **Request**; it never
reaches into the app's state.

**Request**: what a dialog, the message being written, the signature editor,
the reader's viewer, the agenda or the appointment form asks the app to do
after a key, such as closing,
moving mail or saving. The app turns it into an effect and a status line.

**Name prompt**: the dialog asking the name of a new folder, or a new name for
one.

**Move picker**: the dialog picking the folder to move the marked or selected
mail to, narrowed by typing part of its name.

**Folder-pane row**: one row of the folder pane: an account's heading, one of
its folders, or a favorite or unread row showing a folder again. What a row
stands for is asked of the app as a `Row`, never read from its id.

**Mailbox**: one account's cache, read on the UI thread, and the channel to
the core that syncs it. The mailboxes perform the effects the app asks for
and show it the events each core reports.

**Appointment**: something in an account's calendar, such as a meeting or time
kept for oneself. Graph calls it an event; tuilook does not, since an
**Event** is what core tells the UI happened. Each occurrence of a repeating
appointment is an appointment of its own.

**Agenda**: one account's calendar taking over the screen, listing the cached
appointments under the day each starts. It owns its state and answers each key
with a **Request**.

**Appointment form**: the appointment being written or changed from the
agenda. It takes over the agenda until it is saved or closed.
