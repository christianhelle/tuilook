//! Sample mailboxes served by [`FakeBackend`]s for `otlk --demo`.

use std::collections::HashMap;

use crate::core::fake::FakeBackend;
use crate::model::{Body, BodyType, Folder, FolderId, Importance, Message, MessageId, WellKnown};
use jiff::{SignedDuration, Timestamp};

/// The demo's newest mail arrived just before this.
const NOW: &str = "2026-10-24T09:00:00Z";

/// A folder: its id, its parent's id, its name and its Graph role.
type FolderSpec = (
    &'static str,
    Option<&'static str>,
    &'static str,
    Option<WellKnown>,
);

/// A message: its folder, sender, subject, opening line, how many hours
/// before [`NOW`] it arrived, and whether it has been read.
type MessageSpec = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    i64,
    bool,
);

/// Each demo account's address and the mailbox it has.
pub fn accounts() -> Vec<(String, FakeBackend)> {
    vec![
        ("ada@outlook.com".to_owned(), personal()),
        ("ada@analytical.example".to_owned(), work()),
        ("ada@university.example".to_owned(), university()),
    ]
}

fn personal() -> FakeBackend {
    use WellKnown::*;
    mailbox(
        "p",
        &[
            ("inbox", None, "Inbox", Some(Inbox)),
            ("receipts", Some("inbox"), "Receipts", None),
            ("receipts-2025", Some("receipts"), "2025", None),
            ("receipts-2026", Some("receipts"), "2026", None),
            ("travel", Some("inbox"), "Travel", None),
            ("japan", Some("travel"), "Japan 2026", None),
            ("lisbon", Some("travel"), "Lisbon", None),
            ("drafts", None, "Drafts", Some(Drafts)),
            ("sent", None, "Sent Items", Some(SentItems)),
            ("archive", None, "Archive", Some(Archive)),
            ("archive-2024", Some("archive"), "2024", None),
            ("archive-2025", Some("archive"), "2025", None),
            ("deleted", None, "Deleted Items", Some(DeletedItems)),
            ("junk", None, "Junk Email", Some(JunkEmail)),
            ("family", None, "Family", None),
            ("hobbies", None, "Hobbies", None),
            ("photography", Some("hobbies"), "Photography", None),
            ("woodworking", Some("hobbies"), "Woodworking", None),
        ],
        [
            (
                "inbox",
                "Charles Babbage",
                "Dinner on Saturday?",
                "Georgiana is cooking and insists you bring the poems.",
                1,
                false,
            ),
            (
                "inbox",
                "Mary Somerville",
                "Your translation",
                "I read the notes twice; they are longer than the paper, and better.",
                3,
                false,
            ),
            (
                "inbox",
                "Library of London",
                "Books due next week",
                "Three items on your card are due on 31 October.",
                9,
                true,
            ),
            (
                "inbox",
                "Lord Byron",
                "From Venice",
                "The canals are much as I left them, damp and glorious.",
                26,
                true,
            ),
            (
                "inbox",
                "Annabella Milbanke",
                "Sunday visit",
                "Shall we say two o'clock? Bring the children's drawings.",
                30,
                false,
            ),
            (
                "inbox",
                "Bank of England",
                "Your October statement",
                "Your statement for October is ready to view.",
                50,
                true,
            ),
            (
                "inbox",
                "Michael Faraday",
                "Lecture tickets",
                "I kept two seats for the Christmas lecture on candles.",
                75,
                true,
            ),
            (
                "receipts",
                "Wedgwood",
                "Order confirmed",
                "Your order of a teapot (jasperware, blue) is on its way.",
                100,
                true,
            ),
            (
                "receipts-2026",
                "Railway Company",
                "Ticket: London to Brighton",
                "Seat 14, second class, departing 08:15.",
                200,
                true,
            ),
            (
                "receipts-2026",
                "Stationers' Hall",
                "Receipt for paper and ink",
                "Twelve quires of foolscap and two bottles of ink.",
                400,
                true,
            ),
            (
                "receipts-2025",
                "Railway Company",
                "Ticket: London to Bath",
                "Seat 3, first class, departing 07:40.",
                6000,
                true,
            ),
            (
                "travel",
                "Thomas Cook",
                "Itinerary changes",
                "Your excursion now leaves from Leicester at nine.",
                60,
                false,
            ),
            (
                "japan",
                "Kyoto Ryokan",
                "Reservation confirmed",
                "Two nights in a garden room from 12 November.",
                150,
                false,
            ),
            (
                "japan",
                "Japan Airlines",
                "Check-in opens soon",
                "Online check-in for flight JL44 opens 24 hours before.",
                20,
                false,
            ),
            (
                "lisbon",
                "Hotel Avenida",
                "Thank you for staying",
                "We hope you enjoyed the view of the Tagus.",
                3000,
                true,
            ),
            (
                "sent",
                "Ada Lovelace",
                "Re: Dinner on Saturday?",
                "Gladly. I shall bring the poems and a bottle of something.",
                0,
                true,
            ),
            (
                "sent",
                "Ada Lovelace",
                "Notes for Mary",
                "As promised, my notes on the engine's use for music.",
                48,
                true,
            ),
            (
                "archive-2024",
                "Charles Babbage",
                "The Difference Engine, again",
                "Parliament remains unmoved by the merits of the engine.",
                15000,
                true,
            ),
            (
                "archive-2025",
                "Mary Somerville",
                "On the connexion of the sciences",
                "A copy of the new edition is on its way to you.",
                8000,
                true,
            ),
            (
                "deleted",
                "Newsletter",
                "This week in steam",
                "Twelve new locomotives you will not believe.",
                90,
                true,
            ),
            (
                "family",
                "Byron Ockham",
                "Can we get a dog?",
                "Annabella says to ask you. It would be a small dog.",
                12,
                false,
            ),
            (
                "family",
                "Anne Isabella",
                "Pony drawings",
                "I drew the pony again, this time with all four legs.",
                36,
                true,
            ),
            (
                "photography",
                "Henry Fox Talbot",
                "Calotype paper",
                "The new paper takes an image in under a minute.",
                70,
                false,
            ),
            (
                "woodworking",
                "Joiners' Guild",
                "Dovetail workshop",
                "Places are left on the Thursday evening class.",
                120,
                true,
            ),
        ]
        .into_iter()
        .chain(newsletters())
        .chain(spam()),
    )
}

fn work() -> FakeBackend {
    use WellKnown::*;
    mailbox(
        "w",
        &[
            ("inbox", None, "Inbox", Some(Inbox)),
            ("team", Some("inbox"), "Team", None),
            ("standups", Some("team"), "Standups", None),
            ("reviews", Some("team"), "Reviews", None),
            ("notifications", Some("inbox"), "Notifications", None),
            ("ci", Some("notifications"), "Build server", None),
            ("tracker", Some("notifications"), "Issue tracker", None),
            ("drafts", None, "Drafts", Some(Drafts)),
            ("sent", None, "Sent Items", Some(SentItems)),
            ("archive", None, "Archive", Some(Archive)),
            ("deleted", None, "Deleted Items", Some(DeletedItems)),
            ("projects", None, "Projects", None),
            ("engine", Some("projects"), "Analytical Engine", None),
            ("design", Some("engine"), "Design", None),
            ("bugs", Some("engine"), "Bugs", None),
            ("mill", Some("design"), "The Mill", None),
            ("store", Some("design"), "The Store", None),
            ("compiler", Some("projects"), "Compiler", None),
            ("hr", None, "HR", None),
        ],
        [
            (
                "inbox",
                "Charles Babbage",
                "Engine review moved to Thursday",
                "The Duke can only make Thursday, so the review moves.",
                2,
                false,
            ),
            (
                "inbox",
                "Grace Hopper",
                "Compiler draft",
                "First pass of the A-0 compiler is ready for review.",
                4,
                false,
            ),
            (
                "inbox",
                "Alan Turing",
                "Lunch on Friday?",
                "Shall we continue the discussion on computable numbers?",
                7,
                false,
            ),
            (
                "inbox",
                "Joseph Clement",
                "Invoice for the gear trains",
                "Attached is the invoice for the last forty gear trains.",
                22,
                true,
            ),
            (
                "inbox",
                "John von Neumann",
                "Stored programs",
                "What if the program lived in the store with the numbers?",
                28,
                false,
            ),
            (
                "inbox",
                "Facilities",
                "Workshop closed Monday",
                "The workshop is closed on Monday for the boiler inspection.",
                49,
                true,
            ),
            (
                "inbox",
                "Herman Hollerith",
                "Punched cards, bulk order",
                "We can supply ten thousand cards by the end of the month.",
                73,
                true,
            ),
            (
                "standups",
                "Grace Hopper",
                "Standup notes, Thursday",
                "Yesterday: loops. Today: subroutines. Blocked: none.",
                5,
                false,
            ),
            (
                "standups",
                "Alan Turing",
                "Standup notes, Wednesday",
                "Yesterday: the halting problem. Today: still the halting problem.",
                29,
                true,
            ),
            (
                "standups",
                "Grace Hopper",
                "Standup notes, Tuesday",
                "Found a moth in relay 70; taped it into the log book.",
                53,
                true,
            ),
            (
                "reviews",
                "Edsger Dijkstra",
                "Review: goto in the loop control",
                "I must ask that this goto be removed before we merge.",
                10,
                false,
            ),
            (
                "reviews",
                "Donald Knuth",
                "Review: prime sieve",
                "Correct, but see my comments on the inner loop's cost.",
                31,
                true,
            ),
            (
                "ci",
                "Build server",
                "Build 1843 failed",
                "Bernoulli numbers: expected B8 = -1/30, got 1/30.",
                1,
                false,
            ),
            (
                "ci",
                "Build server",
                "Build 1842 passed",
                "All 212 checks passed in 4 minutes 31 seconds.",
                6,
                true,
            ),
            (
                "ci",
                "Build server",
                "Build 1841 passed",
                "All 212 checks passed in 4 minutes 40 seconds.",
                25,
                true,
            ),
            (
                "tracker",
                "Issue tracker",
                "#88 Carry propagates too slowly",
                "Charles Babbage opened an issue in Analytical Engine.",
                8,
                false,
            ),
            (
                "tracker",
                "Issue tracker",
                "#87 Card reader jams in the cold",
                "Joseph Clement closed this issue.",
                54,
                true,
            ),
            (
                "sent",
                "Ada Lovelace",
                "Note G",
                "The engine can compute Bernoulli numbers; table attached.",
                3,
                true,
            ),
            (
                "sent",
                "Ada Lovelace",
                "Re: Compiler draft",
                "Looks great, a few comments inline.",
                30,
                true,
            ),
            (
                "archive",
                "Luigi Menabrea",
                "Sketch of the Analytical Engine",
                "My paper, in French, as discussed. Translate freely.",
                9000,
                true,
            ),
            (
                "deleted",
                "Facilities",
                "Lost: one umbrella",
                "A black umbrella was left in the drawing room.",
                100,
                true,
            ),
            (
                "engine",
                "Charles Babbage",
                "Roadmap for next year",
                "Mill first, then the store, then printing.",
                80,
                true,
            ),
            (
                "mill",
                "Charles Babbage",
                "Anticipating carriage",
                "Here is how the mill carries in a single step.",
                140,
                false,
            ),
            (
                "store",
                "Joseph Clement",
                "A thousand numbers of fifty digits",
                "The store's columns are drawn; see the plans.",
                160,
                true,
            ),
            (
                "bugs",
                "Grace Hopper",
                "Off by one in the loop counter",
                "The loop runs one card too many when the count is zero.",
                15,
                false,
            ),
            (
                "compiler",
                "Grace Hopper",
                "Naming the language",
                "I suggest we call it FLOW-MATIC. Objections?",
                45,
                true,
            ),
            (
                "hr",
                "People Team",
                "Open enrolment",
                "Benefits enrolment closes on 15 November.",
                96,
                true,
            ),
        ],
    )
}

fn university() -> FakeBackend {
    use WellKnown::*;
    mailbox(
        "u",
        &[
            ("inbox", None, "Inbox", Some(Inbox)),
            ("sent", None, "Sent Items", Some(SentItems)),
            ("deleted", None, "Deleted Items", Some(DeletedItems)),
            ("courses", None, "Courses", None),
            ("maths", Some("courses"), "Mathematics", None),
            ("calculus", Some("maths"), "Calculus", None),
            ("numbers", Some("maths"), "Number theory", None),
            ("computing", Some("courses"), "Computing", None),
            ("research", None, "Research", None),
            ("papers", Some("research"), "Papers", None),
            ("drafts-research", Some("papers"), "In progress", None),
            ("conferences", Some("research"), "Conferences", None),
        ],
        [
            (
                "inbox",
                "Augustus De Morgan",
                "Your exercises",
                "Your solutions to the series are correct, with one quibble.",
                11,
                false,
            ),
            (
                "inbox",
                "Registry",
                "Term dates",
                "Michaelmas term ends on 6 December.",
                60,
                true,
            ),
            (
                "inbox",
                "Royal Society",
                "Lecture invitation",
                "You are invited to the Bakerian Lecture on 20 November.",
                80,
                false,
            ),
            (
                "calculus",
                "Augustus De Morgan",
                "Limits, again",
                "Consider again the limit I sent last week; it is subtler.",
                18,
                false,
            ),
            (
                "calculus",
                "Augustus De Morgan",
                "Problem set 4",
                "Problems on infinite series, due next Tuesday.",
                110,
                true,
            ),
            (
                "numbers",
                "Sophie Germain",
                "Primes of a special form",
                "I believe p and 2p + 1 both prime deserve a name.",
                33,
                false,
            ),
            (
                "computing",
                "Charles Babbage",
                "Guest lecture",
                "Would you speak to the students about the engine?",
                64,
                true,
            ),
            (
                "papers",
                "Philosophical Magazine",
                "Manuscript accepted",
                "Your note on the engine will appear in the next issue.",
                200,
                true,
            ),
            (
                "drafts-research",
                "Mary Somerville",
                "Comments on your draft",
                "Section 3 could say more about the cards' looping.",
                24,
                false,
            ),
            (
                "conferences",
                "British Association",
                "Call for papers",
                "Submissions for the Cambridge meeting close 1 December.",
                140,
                false,
            ),
            (
                "sent",
                "Ada Lovelace",
                "Re: Your exercises",
                "Thank you; I have taken your quibble to heart.",
                10,
                true,
            ),
        ],
    )
}

/// Who sends the newsletters, and the issues each has sent, newest first.
const NEWSLETTERS: &[(&str, &[(&str, &str)])] = &[
    (
        "Royal Institution",
        &[
            (
                "Friday Evening Discourse",
                "This week: the chemistry of a candle flame.",
            ),
            (
                "New members' evening",
                "Meet the fellows over tea in the library.",
            ),
            (
                "Lecture notes now online",
                "Last month's lectures are ready to read.",
            ),
            (
                "Christmas lectures announced",
                "Tickets for the children's lectures open soon.",
            ),
            (
                "The year in science",
                "A look back at twelve months of discovery.",
            ),
            (
                "Volunteer guides wanted",
                "Show visitors round the laboratory on Saturdays.",
            ),
            (
                "Members' magazine",
                "In this issue: electricity, magnetism and you.",
            ),
            (
                "Reminder: subscriptions",
                "Your annual subscription renews next month.",
            ),
        ],
    ),
    (
        "The Mechanics' Magazine",
        &[
            (
                "Weekly digest",
                "Steam hammers, safety valves and a new lathe.",
            ),
            (
                "Readers' inventions",
                "A self-winding clock and a mechanical duck.",
            ),
            ("Patent roundup", "Forty-one new patents granted this week."),
            (
                "Letters to the editor",
                "On the proper oiling of a beam engine.",
            ),
            (
                "Weekly digest",
                "Iron ships, gas lamps and a calculating machine.",
            ),
            ("Workshop tips", "How to true a grindstone in five minutes."),
            ("Weekly digest", "Rail gauges compared, and why it matters."),
            (
                "Special issue: engines",
                "Everything you wanted to know about governors.",
            ),
        ],
    ),
    (
        "Kew Gardens",
        &[
            (
                "Autumn in the gardens",
                "The maples are turning; come before they fall.",
            ),
            (
                "Palm House reopens",
                "The glasshouse is open again after repairs.",
            ),
            (
                "Members' plant sale",
                "Cuttings from the collection, Saturday morning.",
            ),
            (
                "Seed exchange",
                "Bring seeds from your garden to swap with others.",
            ),
            (
                "Orchid festival",
                "Two hundred orchids on show for one week only.",
            ),
            (
                "Guided walks",
                "Learn the trees of the arboretum with a botanist.",
            ),
            (
                "Winter opening hours",
                "From November the gates close at four.",
            ),
            (
                "New arrivals",
                "Rare ferns collected in New Zealand now on display.",
            ),
        ],
    ),
    (
        "The Gazette",
        &[
            (
                "Morning briefing",
                "Parliament sits late, and the price of corn falls.",
            ),
            (
                "Weekend reading",
                "Long reads on travel, science and the arts.",
            ),
            (
                "Morning briefing",
                "A new bridge for the Thames is approved.",
            ),
            ("Morning briefing", "Fog delays trains across the south."),
            ("Weekend reading", "The best new novels of the season."),
            ("Morning briefing", "The Queen opens a hospital in Bristol."),
            ("Morning briefing", "Telegraph lines reach further north."),
            ("Weekend reading", "A walking tour of the Lake District."),
        ],
    ),
    (
        "Fortnum's",
        &[
            (
                "Hampers for Christmas",
                "Order early to be sure of delivery by the 20th.",
            ),
            (
                "New teas from Assam",
                "Three new single-estate teas are in store.",
            ),
            (
                "Your points balance",
                "You have points to spend before the year ends.",
            ),
            (
                "Tasting evening",
                "Cheese and wine in the shop on Thursday.",
            ),
            (
                "Marmalade season",
                "Seville oranges have arrived; jars on sale soon.",
            ),
            (
                "Free delivery this week",
                "No delivery charge on orders over a guinea.",
            ),
            (
                "Biscuit of the month",
                "A ginger biscuit to make the autumn bearable.",
            ),
            (
                "Thank you for your order",
                "We hope you enjoyed the shortbread.",
            ),
        ],
    ),
];

/// Every newsletter's issues, interleaved and half a day apart, so the
/// personal inbox is long enough to scroll.
fn newsletters() -> impl Iterator<Item = MessageSpec> {
    let issues = NEWSLETTERS.iter().map(|(_, issues)| issues.len()).max();
    (0..issues.unwrap_or(0))
        .flat_map(|issue| NEWSLETTERS.iter().map(move |sender| (issue, sender)))
        .filter_map(|(issue, (from, issues))| Some((*from, issues.get(issue)?)))
        .enumerate()
        .map(|(i, (from, &(subject, preview)))| {
            ("inbox", from, subject, preview, 4 + 13 * i as i64, i > 3)
        })
}

/// Who sends the junk.
const SPAMMERS: &[&str] = &[
    "Prince of Ruritania",
    "Miracle Tonics",
    "Lottery Office",
    "Dr. Bartholomew's Remedies",
    "Gold Mine Shares Ltd",
    "Secret Admirer",
    "Bank Security Dept",
    "Free Railway Tickets",
    "Phrenology Institute",
    "Perpetual Motion Co",
    "Widow of a Sea Captain",
    "Discount Snuff",
    "Clairvoyant Madame Zora",
];

/// What the junk is about: a subject and an opening line.
const PITCHES: &[(&str, &str)] = &[
    (
        "An urgent business proposal",
        "I write to you in strictest confidence about a fortune.",
    ),
    (
        "Feel twenty years younger",
        "Our elixir is trusted by thousands of satisfied customers.",
    ),
    (
        "You have won!!!",
        "Claim your prize of ten thousand guineas before Friday.",
    ),
    (
        "Your account is suspended",
        "Confirm your details at once to keep your savings safe.",
    ),
    (
        "Invest in a sure thing",
        "Shares in our Peruvian silver mine can only go up.",
    ),
    (
        "Someone fancies you",
        "A lady of means has noticed you at the opera.",
    ),
    (
        "Final notice",
        "This is your last chance to settle the invoice enclosed.",
    ),
    (
        "Free first-class travel",
        "Answer three questions and ride anywhere in the kingdom.",
    ),
    (
        "Know your true character",
        "Send us a sketch of your skull for a full reading.",
    ),
    (
        "The machine that never stops",
        "Free power forever, patent pending, investors wanted.",
    ),
    (
        "A sad inheritance",
        "My late husband left a chest of pearls with no heir.",
    ),
    (
        "Half price, today only",
        "Every tin in the shop at half price until midnight.",
    ),
    (
        "Your future revealed",
        "The cards have spoken; a stranger brings great news.",
    ),
    (
        "Re: your parcel",
        "We could not deliver your parcel. Pay the fee to rebook.",
    ),
    (
        "Lose a stone in a week",
        "No diet, no exercise, just one spoon before bed.",
    ),
    (
        "Exclusive offer for you",
        "Selected customers only: a gift is waiting to be claimed.",
    ),
    (
        "Act now",
        "Prices rise tomorrow. Reply with your bank details.",
    ),
    (
        "Congratulations, Ada",
        "You have been chosen for a once-in-a-lifetime opportunity.",
    ),
    (
        "Work from home",
        "Earn a pound a day folding letters from your parlour.",
    ),
    (
        "Unclaimed funds in your name",
        "A sum has lain unclaimed at the Bank for forty years.",
    ),
];

/// A hundred junk mails, a few hours apart, mixing every spammer with
/// every pitch, so the Junk Email folder is long enough to scroll.
fn spam() -> impl Iterator<Item = MessageSpec> {
    (0..100).map(|i| {
        let (subject, preview) = PITCHES[i % PITCHES.len()];
        (
            "junk",
            SPAMMERS[i % SPAMMERS.len()],
            subject,
            preview,
            5 + 3 * i as i64,
            i % 4 == 3,
        )
    })
}

/// A mailbox with `folders` holding `messages`, with every id prefixed by
/// `key` so that no two accounts share one.
fn mailbox(
    key: &str,
    folders: &[FolderSpec],
    messages: impl IntoIterator<Item = MessageSpec>,
) -> FakeBackend {
    let id = |local: &str| FolderId(format!("{key}-{local}"));
    let now: Timestamp = NOW.parse().expect("valid demo timestamp");
    let mut by_folder: HashMap<FolderId, Vec<Message>> = HashMap::new();
    for (index, (folder, from, subject, preview, hours_ago, is_read)) in
        messages.into_iter().enumerate()
    {
        let message = Message {
            id: MessageId(format!("{key}-m{index}")),
            folder_id: id(folder),
            conversation_id: format!("{key}-c{index}"),
            subject: subject.to_owned(),
            from_name: from.to_owned(),
            from_addr: address_of(from),
            received_at: now - SignedDuration::from_hours(hours_ago) - SignedDuration::from_mins(7),
            is_read,
            flagged: false,
            pinned: false,
            importance: Importance::Normal,
            has_attachments: false,
            preview: preview.to_owned(),
        };
        by_folder
            .entry(message.folder_id.clone())
            .or_default()
            .push(message);
    }
    let folders = folders
        .iter()
        .map(|&(local, parent, name, well_known)| {
            let in_it = by_folder.get(&id(local)).map_or(&[][..], Vec::as_slice);
            Folder {
                id: id(local),
                // Graph puts top folders under a root folder of its own.
                parent_id: Some(parent.map_or_else(|| FolderId(format!("{key}-root")), id)),
                name: name.to_owned(),
                well_known,
                unread: in_it.iter().filter(|m| !m.is_read).count() as u32,
                total: in_it.len() as u32,
            }
        })
        .collect();
    let bodies = by_folder
        .values()
        .flatten()
        .map(|m| (m.id.clone(), body(m)))
        .collect();
    FakeBackend {
        folders,
        bodies,
        messages: by_folder,
        ..Default::default()
    }
}

/// A plain address made from the sender's name.
fn address_of(name: &str) -> String {
    let local: String = name
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(".")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.')
        .collect();
    format!("{local}@example.com")
}

/// An HTML body that opens with the preview, with a list, a link and a
/// sign-off, to show how HTML mail reads as text.
fn body(message: &Message) -> Body {
    let first = message
        .from_name
        .split(' ')
        .next()
        .unwrap_or(&message.from_name);
    let html = format!(
        r#"<p>Hi Ada,</p>
<p>{preview}</p>
<p>A few things before we next speak:</p>
<ul><li>whether the timing still suits you,</li><li>what you would change,</li><li>who else should hear of it.</li></ul>
<p>The details are at <a href="https://example.com/{id}">example.com</a>.</p>
<p>Best wishes,<br>{first}</p>"#,
        preview = message.preview,
        id = message.id.0,
    );
    Body {
        content_type: BodyType::Html,
        content: html,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn no_two_accounts_share_a_folder_or_message_id() {
        let mut folders = HashSet::new();
        let mut messages = HashSet::new();
        for (_, backend) in accounts() {
            for folder in &backend.folders {
                assert!(folders.insert(folder.id.clone()), "{:?} twice", folder.id);
            }
            for message in backend.messages.values().flatten() {
                assert!(
                    messages.insert(message.id.clone()),
                    "{:?} twice",
                    message.id
                );
            }
        }
    }

    #[test]
    fn the_personal_junk_folder_holds_a_hundred_mails() {
        let (_, personal) = &accounts()[0];
        let junk = &personal.messages[&FolderId("p-junk".to_owned())];
        assert_eq!(junk.len(), 100);
    }

    #[test]
    fn the_personal_inbox_is_long_enough_to_scroll() {
        let (_, personal) = &accounts()[0];
        let inbox = &personal.messages[&FolderId("p-inbox".to_owned())];
        assert!(inbox.len() >= 40, "only {} in the inbox", inbox.len());
    }

    #[test]
    fn every_message_is_in_a_folder_and_has_a_body() {
        for (address, backend) in accounts() {
            for (folder, messages) in &backend.messages {
                assert!(
                    backend.folders.iter().any(|f| &f.id == folder),
                    "{address} has no folder {folder:?}"
                );
                for message in messages {
                    assert!(backend.bodies.contains_key(&message.id));
                }
            }
        }
    }
}
