//! Things you say to parley rather than to Claude: short commands that press keys, and
//! asking for the answer out loud.

use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum Command {
    /// Esc: interrupt Claude.
    Stop,
    /// Esc twice: Claude Code's rewind to an earlier message.
    Rewind,
    /// A slash command, typed and sent.
    Slash(&'static str),
}

impl Command {
    /// For the overlay.
    pub fn describe(&self) -> &'static str {
        match self {
            Command::Stop => "Esc",
            Command::Rewind => "Esc Esc (rewind)",
            Command::Slash(slash) => slash,
        }
    }
}

/// The whole turn is a command ("stop.", "clear the chat"), not a sentence that happens
/// to contain one.
pub fn command(said: &str) -> Option<Command> {
    let words: String = said
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '\'' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let words = words
        .trim_start_matches("okay ")
        .trim_start_matches("ok ")
        .trim_start_matches("please ");
    let words = words.trim_end_matches(" please");
    Some(match words {
        "stop" | "stop it" | "stop that" | "stop stop" | "cancel" | "cancel that" | "hold on"
        | "wait" | "wait wait" | "interrupt" | "escape" => Command::Stop,
        "undo" | "undo that" | "go back" | "rewind" | "take that back" => Command::Rewind,
        "clear"
        | "clear it"
        | "clear the chat"
        | "clear the context"
        | "clear the conversation"
        | "new chat"
        | "new conversation"
        | "start over" => Command::Slash("/clear"),
        "compact"
        | "compact it"
        | "compact the chat"
        | "compact the context"
        | "compact the conversation" => Command::Slash("/compact"),
        _ => return None,
    })
}

/// You asked to hear the answer: "answer me out loud", "tell me by voice", "read it to
/// me", or a turn that ends with "answer me".
static WANTS_VOICE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        \b(answer|reply|respond|tell|explain|say|read|talk)\b [^.?!]{0,40}
            \b(out\ loud|aloud|by\ voice|in\ (a\ )?voice|with\ (your|a)\ voice|verbally|in\ speech|in\ audio)\b
        | \b(voice|spoken|audio)\ (answer|reply|response)\b
        | \b(read|say)\ (it|that|this|the\ answer)\ (to\ me|back)\b
        | \btalk\ me\ through\b
        | \b(answer|reply\ to|tell)\ me\W*$",
    )
    .unwrap()
});

pub fn wants_voice(said: &str) -> bool {
    WANTS_VOICE.is_match(said)
}

/// Appended to a prompt whose answer will be spoken, so Claude writes something that
/// sounds right read aloud.
pub fn voice_note(language: &str) -> String {
    let lang = if language == "English" {
        String::new()
    } else {
        format!(" Answer in {language}.")
    };
    format!(
        " (Your reply will be read aloud to me by text-to-speech: answer in a few short \
         spoken sentences, without code blocks, lists, tables or file paths.{lang})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_turn_commands() {
        assert_eq!(command("Stop."), Some(Command::Stop));
        assert_eq!(
            command("Okay, clear the chat."),
            Some(Command::Slash("/clear"))
        );
        assert_eq!(command("Compact"), Some(Command::Slash("/compact")));
        assert_eq!(command("undo that"), Some(Command::Rewind));
        assert_eq!(command("Stop using the old API in the auth module."), None);
    }

    #[test]
    fn asking_for_a_spoken_answer() {
        assert!(wants_voice(
            "What does this function do? Answer me out loud."
        ));
        assert!(wants_voice("Explain the build steps by voice"));
        assert!(wants_voice("Why is the test failing, answer me."));
        assert!(wants_voice("Give me a voice answer: is it safe to merge?"));
        assert!(!wants_voice(
            "Answer the user's question in the reply field."
        ));
        assert!(!wants_voice("Fix the loud fan warning in the logs."));
    }
}
