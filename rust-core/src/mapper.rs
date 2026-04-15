use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MapResult {
    pub text: String,
    pub corrected_words: HashSet<String>,
}

pub struct TechAcronymMapper {
    user_corrections: HashMap<String, String>,
    corrections: HashMap<&'static str, &'static str>,
}

impl TechAcronymMapper {
    pub fn new() -> Self {
        Self {
            user_corrections: HashMap::new(),
            corrections: HashMap::from([
                ("clawd", "Claude"),
                ("clawd code", "Claude Code"),
                ("clawed", "Claude"),
                ("clawed code", "Claude Code"),
                ("cloud code", "Claude Code"),
                ("clod code", "Claude Code"),
                ("chat gpt", "ChatGPT"),
                ("chat g p t", "ChatGPT"),
                ("g p t", "GPT"),
                ("open ai", "OpenAI"),
                ("open a i", "OpenAI"),
                ("anthropic", "Anthropic"),
                ("co pilot", "Copilot"),
                ("github co pilot", "GitHub Copilot"),
                ("lama", "LLaMA"),
                ("llama", "LLaMA"),
                ("hugging face", "Hugging Face"),
                ("k8s", "Kubernetes"),
                ("kube", "Kubernetes"),
                ("kubernetes", "Kubernetes"),
                ("coober netties", "Kubernetes"),
                ("cooper netties", "Kubernetes"),
                ("vpc", "VPC"),
                ("iam", "IAM"),
                ("i am", "IAM"),
                ("s3", "S3"),
                ("s three", "S3"),
                ("ec2", "EC2"),
                ("e c two", "EC2"),
                ("rds", "RDS"),
                ("sqs", "SQS"),
                ("sns", "SNS"),
                ("lambda", "Lambda"),
                ("dynamo db", "DynamoDB"),
                ("dynamo d b", "DynamoDB"),
                ("terraform", "Terraform"),
                ("terra form", "Terraform"),
                ("prometheus", "Prometheus"),
                ("grafana", "Grafana"),
                ("docker", "Docker"),
                ("helm", "Helm"),
                ("nginx", "NGINX"),
                ("engine x", "NGINX"),
                ("kotlin", "Kotlin"),
                ("java", "Java"),
                ("javascript", "JavaScript"),
                ("java script", "JavaScript"),
                ("typescript", "TypeScript"),
                ("type script", "TypeScript"),
                ("python", "Python"),
                ("rust", "Rust"),
                ("react", "React"),
                ("angular", "Angular"),
                ("vue", "Vue"),
                ("next js", "Next.js"),
                ("next j s", "Next.js"),
                ("node js", "Node.js"),
                ("node j s", "Node.js"),
                ("spring boot", "Spring Boot"),
                ("flask", "Flask"),
                ("django", "Django"),
                ("jango", "Django"),
                ("github", "GitHub"),
                ("git hub", "GitHub"),
                ("gitlab", "GitLab"),
                ("git lab", "GitLab"),
                ("bitbucket", "Bitbucket"),
                ("jira", "Jira"),
                ("confluence", "Confluence"),
                ("slack", "Slack"),
                ("vs code", "VS Code"),
                ("vscode", "VS Code"),
                ("intellij", "IntelliJ"),
                ("intelli j", "IntelliJ"),
                ("android studio", "Android Studio"),
                ("xcode", "Xcode"),
                ("x code", "Xcode"),
                ("postgres", "PostgreSQL"),
                ("postgre sql", "PostgreSQL"),
                ("mongo db", "MongoDB"),
                ("redis", "Redis"),
                ("elastic search", "Elasticsearch"),
                ("kafka", "Kafka"),
                ("rabbit mq", "RabbitMQ"),
                ("api", "API"),
                ("a p i", "API"),
                ("rest", "REST"),
                ("graphql", "GraphQL"),
                ("graph q l", "GraphQL"),
                ("grpc", "gRPC"),
                ("g r p c", "gRPC"),
                ("http", "HTTP"),
                ("https", "HTTPS"),
                ("tcp", "TCP"),
                ("udp", "UDP"),
                ("websocket", "WebSocket"),
                ("web socket", "WebSocket"),
                ("oauth", "OAuth"),
                ("o auth", "OAuth"),
                ("jwt", "JWT"),
                ("json", "JSON"),
                ("j son", "JSON"),
                ("yaml", "YAML"),
                ("xml", "XML"),
                ("sql", "SQL"),
                ("sequel", "SQL"),
                ("css", "CSS"),
                ("html", "HTML"),
                ("ci cd", "CI/CD"),
                ("ci/cd", "CI/CD"),
                ("stt", "STT"),
                ("tts", "TTS"),
                ("asr", "ASR"),
                ("nlp", "NLP"),
                ("ml", "ML"),
                ("ai", "AI"),
                ("a i", "AI"),
                ("llm", "LLM"),
                ("rag", "RAG"),
                ("sdk", "SDK"),
                ("cli", "CLI"),
                ("gui", "GUI"),
                ("ide", "IDE"),
                ("orm", "ORM"),
                ("dns", "DNS"),
                ("cdn", "CDN"),
                ("ssl", "SSL"),
                ("tls", "TLS"),
                ("ssh", "SSH"),
                ("vm", "VM"),
                ("os", "OS"),
                ("cpu", "CPU"),
                ("gpu", "GPU"),
                ("ram", "RAM"),
                ("ssd", "SSD"),
                ("sherpa", "Sherpa"),
                ("sherpa onyx", "Sherpa-ONNX"),
                ("sherpa on x", "Sherpa-ONNX"),
                ("onnx", "ONNX"),
                ("on x", "ONNX"),
                ("compose", "Compose"),
                ("jetpack compose", "Jetpack Compose"),
                ("multiplatform", "Multiplatform"),
                ("multi platform", "Multiplatform"),
            ]),
        }
    }

    pub fn add_correction(&mut self, wrong: &str, correct: &str) {
        self.user_corrections
            .insert(wrong.to_ascii_lowercase(), correct.to_owned());
    }

    pub fn load_user_corrections_file(&mut self, path: impl AsRef<Path>) -> std::io::Result<usize> {
        let content = fs::read_to_string(path)?;
        let mut loaded = 0usize;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let separator = if line.contains("=>") {
                "=>"
            } else if line.contains('=') {
                "="
            } else {
                continue;
            };

            let mut parts = line.splitn(2, separator);
            let Some(wrong) = parts.next().map(str::trim).filter(|s| !s.is_empty()) else {
                continue;
            };
            let Some(correct) = parts.next().map(str::trim).filter(|s| !s.is_empty()) else {
                continue;
            };

            self.add_correction(wrong, correct);
            loaded += 1;
        }

        Ok(loaded)
    }

    pub fn map(&self, text: &str) -> String {
        self.map_with_info(text).text
    }

    pub fn map_with_info(&self, text: &str) -> MapResult {
        let mut processed = text.to_owned();
        let mut corrected_words = HashSet::new();

        let mut combined: Vec<(String, String)> = self
            .corrections
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();

        combined.extend(
            self.user_corrections
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );

        combined.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));

        for (phrase, replacement) in combined {
            if let Some(updated) =
                replace_case_insensitive_whole_phrase(&processed, &phrase, &replacement)
            {
                processed = updated;
                corrected_words.insert(replacement);
            }
        }

        processed = normalize_spoken_symbols(&processed);

        MapResult {
            text: processed,
            corrected_words,
        }
    }
}

impl Default for TechAcronymMapper {
    fn default() -> Self {
        Self::new()
    }
}

fn replace_case_insensitive_whole_phrase(
    input: &str,
    phrase: &str,
    replacement: &str,
) -> Option<String> {
    let lower = input.to_ascii_lowercase();
    let needle = phrase.to_ascii_lowercase();
    let mut matches = Vec::new();
    let mut offset = 0usize;

    while let Some(found) = lower[offset..].find(&needle) {
        let start = offset + found;
        let end = start + needle.len();
        let left_ok = start == 0 || !is_word_char(lower.as_bytes()[start - 1]);
        let right_ok = end == lower.len() || !is_word_char(lower.as_bytes()[end]);

        if left_ok && right_ok {
            matches.push((start, end));
        }

        offset = end;
        if offset >= lower.len() {
            break;
        }
    }

    if matches.is_empty() {
        return None;
    }

    let mut result = String::with_capacity(input.len());
    let mut cursor = 0usize;
    for (start, end) in matches {
        result.push_str(&input[cursor..start]);
        result.push_str(replacement);
        cursor = end;
    }
    result.push_str(&input[cursor..]);
    Some(result)
}

fn is_word_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn normalize_spoken_symbols(input: &str) -> String {
    let collapsed = collapse_double_dash_tokens(input);
    let joined = join_symbol_runs(&collapsed);
    lowercase_domain_tokens(&joined)
}

fn collapse_double_dash_tokens(input: &str) -> String {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let mut collapsed = Vec::with_capacity(parts.len());
    let mut index = 0usize;

    while index < parts.len() {
        if index + 1 < parts.len() && parts[index] == "-" && parts[index + 1] == "-" {
            collapsed.push(String::from("--"));
            index += 2;
            continue;
        }

        collapsed.push(parts[index].to_owned());
        index += 1;
    }

    collapsed.join(" ")
}

fn join_symbol_runs(input: &str) -> String {
    let parts: Vec<&str> = input.split_whitespace().collect();
    let mut output = String::new();
    let mut suppress_space_before_next = false;

    for index in 0..parts.len() {
        let token = parts[index];
        let previous = if index > 0 {
            Some(parts[index - 1])
        } else {
            None
        };
        let next = parts.get(index + 1).copied();

        if token == "--" && previous.is_some() && next.is_some_and(is_joinable_atom) {
            if !output.is_empty() && !output.ends_with(' ') {
                output.push(' ');
            }
            output.push_str("--");
            suppress_space_before_next = true;
            continue;
        }

        if is_inline_connector(token)
            && previous.is_some_and(is_joinable_atom)
            && next.is_some_and(is_joinable_atom)
        {
            output.push_str(token);
            suppress_space_before_next = true;
            continue;
        }

        if !output.is_empty() && !suppress_space_before_next {
            output.push(' ');
        }
        output.push_str(token);
        suppress_space_before_next = false;
    }

    output
}

fn lowercase_domain_tokens(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            if looks_like_domain(token) {
                token.to_ascii_lowercase()
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_inline_connector(token: &str) -> bool {
    matches!(token, "." | "/" | "_" | ":" | "-")
}

fn is_joinable_atom(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn looks_like_domain(token: &str) -> bool {
    token.contains('.')
        && !token.contains('/')
        && !token.contains('@')
        && token.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::TechAcronymMapper;

    #[test]
    fn maps_double_dash_flag_compounds() {
        let mut mapper = TechAcronymMapper::new();
        mapper.add_correction("dash", "-");
        assert_eq!(mapper.map("shell dash dash help"), "shell --help");
    }

    #[test]
    fn maps_domain_style_dot_compounds() {
        let mut mapper = TechAcronymMapper::new();
        mapper.add_correction("dot", ".");
        mapper.add_correction("w w w", "WWW");
        assert_eq!(mapper.map("w w w dot aol dot com"), "www.aol.com");
    }
}
