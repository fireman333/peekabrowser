use super::Destination;

pub fn default_destinations() -> Vec<Destination> {
    vec![
        Destination {
            id: "google".to_string(),
            name: "Google".to_string(),
            url: "https://www.google.com".to_string(),
            icon: "".to_string(),
            order: 0,
        },
        Destination {
            id: "chatgpt".to_string(),
            name: "ChatGPT".to_string(),
            url: "https://chat.openai.com".to_string(),
            icon: "".to_string(),
            order: 1,
        },
        Destination {
            id: "claude".to_string(),
            name: "Claude".to_string(),
            url: "https://claude.ai".to_string(),
            icon: "".to_string(),
            order: 2,
        },
        Destination {
            id: "gemini".to_string(),
            name: "Gemini".to_string(),
            url: "https://gemini.google.com".to_string(),
            icon: "".to_string(),
            order: 3,
        },
        Destination {
            id: "perplexity".to_string(),
            name: "Perplexity".to_string(),
            url: "https://www.perplexity.ai".to_string(),
            icon: "".to_string(),
            order: 4,
        },
        Destination {
            id: "openevidence".to_string(),
            name: "OpenEvidence".to_string(),
            url: "https://www.openevidence.com".to_string(),
            icon: "".to_string(),
            order: 5,
        },
    ]
}
