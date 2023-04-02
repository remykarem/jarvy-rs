use std::{
    collections::VecDeque,
    process::{Command, Stdio},
};

pub struct TtsAssistant {
    sentence_buffer: VecDeque<String>,
    process: std::process::Child,
}

impl Default for TtsAssistant {
    fn default() -> Self {
        Self {
            sentence_buffer: VecDeque::new(),
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
    pub fn push(&mut self, chars: &[char]) {
        self.sentence_buffer.push_back(chars.iter().collect());
    }
}
