# Glossary

Words the code and its docs use for tuilook's own ideas.

**Dialog**: a box over the panes that takes every key until it closes:
the help, a blocked send, a confirmation, the name prompt or the move picker.
A dialog owns its state and answers each key with a **Request**; it never
reaches into the app's state.

**Request**: what a dialog, the message being written, the signature editor
or the reader's viewer asks the app to do after a key, such as closing,
moving mail or saving. The app turns it into an effect and a status line.

**Name prompt**: the dialog asking the name of a new folder, or a new name for
one.

**Move picker**: the dialog picking the folder to move the marked or selected
mail to, narrowed by typing part of its name.
