use crate::model::MessageId;

/// What a new message answers, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeKind {
    New,
    Reply(MessageId),
    /// Reply to the sender and everyone else on the message.
    ReplyAll(MessageId),
    Forward(MessageId),
}

/// A message written in tuilook, ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub kind: ComposeKind,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    /// Plain text. A reply or forward gets the original quoted below it.
    pub body: String,
}

/// Splits a list of addresses typed by hand, separated by commas or semicolons.
pub fn parse_addresses(text: &str) -> Vec<String> {
    text.split([',', ';'])
        .map(str::trim)
        .filter(|address| !address.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_split_on_commas_and_semicolons() {
        assert_eq!(
            parse_addresses(" ada@example.com, grace@example.com;alan@example.com ,"),
            ["ada@example.com", "grace@example.com", "alan@example.com"]
        );
        assert!(parse_addresses("  ").is_empty());
    }
}
