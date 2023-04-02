use std::{
    collections::VecDeque,
    process::{Command, Stdio},
};

pub struct TtsAssistant {
    sentence_buffer: VecDeque<String>,
    pub char_buffer: Vec<char>,
    process: std::process::Child,
}

impl Default for TtsAssistant {
    fn default() -> Self {
        Self {
            sentence_buffer: VecDeque::new(),
            char_buffer: Vec::new(),
            process: Command::new("echo").stdout(Stdio::piped()).spawn().unwrap(),
        }
    }
}

impl TtsAssistant {
    fn say(&mut self, sentence: &str) {
        self.process.wait().unwrap();
        self.process = Command::new("say")
            .arg("-r")
            .arg("200")
            .arg("-v")
            .arg("samantha")
            .arg(sentence)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
    }
    pub fn flush(&mut self) {
        while let Some(sentence) = self.sentence_buffer.pop_front() {
            self.say(&sentence);
        }
    }
    pub fn process_token(&mut self, token: &str) {
        if token.starts_with(&['.', ':', '\n', '!', '?'][..])
            || token.ends_with(&['.', ':', '\n', '!', '?'][..])
        {
            // Concatenate, then say the sentence using macOS's say command
            let sentence = self.char_buffer.iter().collect::<String>();
            self.char_buffer.clear();

            self.sentence_buffer.push_back(sentence);

            if let Some(_exit_status) = self.process.try_wait().unwrap() {
                if !self.sentence_buffer.is_empty() {
                    let say_sentence = self
                        .sentence_buffer
                        .drain(..)
                        .collect::<Vec<String>>()
                        .join("");
                    self.say(&say_sentence);
                    self.sentence_buffer.clear();
                }
            }
        } else {
            // If the token is not a period, append it to the buffer
            self.char_buffer.extend(token.chars());
        }
    }
    pub fn last_char_in_buffer(&self) -> Option<char> {
        self.char_buffer.last().copied()
    }
}
