use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;

pub struct CodeAssistant {
    char_buffer: Vec<char>,
    snippets_buffer: VecDeque<String>,
    home_dir: PathBuf,
}

impl CodeAssistant {
    pub fn new(home_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&home_dir).expect("Could not create home directory");
        Self {
            char_buffer: Vec::new(),
            snippets_buffer: VecDeque::new(),
            home_dir,
        }
    }
}

impl CodeAssistant {
    pub fn flush(&mut self) {
        if !self.char_buffer.is_empty() {
            self.snippets_buffer
                .push_back(self.char_buffer.iter().collect());
            self.char_buffer.clear();
        }
        self.flush_code_snippets();
    }
    pub fn push(&mut self, chars: &[char]) {
        self.snippets_buffer.push_back(chars.iter().collect());
    }
    fn flush_code_snippets(&mut self) {
        while let Some(code_snippet) = self.snippets_buffer.pop_front() {
            let mut lines = code_snippet.lines();
            let language_and_filename = lines.next().unwrap().trim_start_matches("```").to_string();
            let mut file_details = language_and_filename
                .split('-')
                .map(|s| s.trim())
                .collect::<Vec<_>>();

            let (language, filename) = if file_details.len() == 2 {
                (file_details.remove(0), Some(file_details.remove(0)))
            } else {
                (file_details[0], None)
            };

            let code = lines.collect::<Vec<_>>().join("\n");

            if let Some(filename) = filename {
                let filepath = self.home_dir.join(filename);
                std::fs::write(filepath, code).unwrap();
            } else {
                let mut is_file = String::new();
                loop {
                    println!("file or shell or drop? ");
                    std::io::stdin().read_line(&mut is_file).unwrap();
                    is_file = is_file.trim().to_string();
                    if is_file == "file" {
                        let mut filename = String::new();
                        println!("filename? ");
                        std::io::stdin().read_line(&mut filename).unwrap();
                        let filepath = self.home_dir.join(filename.trim());
                        std::fs::write(filepath, code).unwrap();
                        break;
                    } else if is_file == "shell" {
                        // Blocking call
                        let output = Command::new("sh").arg("-c").arg(&code).output().unwrap();
                        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let _stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        let exit_code = output.status.code().unwrap_or(-1);
                        println!("{} {}", exit_code, stdout);

                        // Break so that we can give (negative) feedback to the model
                        // Otherwise, the model will keep on generating
                        // and we won't be able to see the output
                        if exit_code == 0 {
                            // Continue execution
                        } else {
                            break;
                        }
                    } else {
                        // Keep asking for input
                    }
                }
            }
        }
    }
}