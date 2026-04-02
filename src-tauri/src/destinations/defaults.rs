use super::Destination;

fn dest(id: &str, name: &str, url: &str, icon: &str, order: usize) -> Destination {
    Destination {
        id: id.to_string(),
        name: name.to_string(),
        url: url.to_string(),
        icon: icon.to_string(),
        order,
        clip_prompt: String::new(),
    }
}

pub fn default_destinations() -> Vec<Destination> {
    vec![
        dest("google", "Google", "https://www.google.com", "", 0),
        dest("chatgpt", "ChatGPT", "https://chat.openai.com", "", 1),
        dest("claude", "Claude", "https://claude.ai", "", 2),
        dest("gemini", "Gemini", "https://gemini.google.com", "", 3),
        dest("perplexity", "Perplexity", "https://www.perplexity.ai", "", 4),
        dest("openevidence", "OpenEvidence", "https://www.openevidence.com", "", 5),
        dest("system-calendar", "Calendar", "system://calendar", "📅", 10),
        dest("system-reminders", "Reminders", "system://reminders", "☑️", 11),
    ]
}
