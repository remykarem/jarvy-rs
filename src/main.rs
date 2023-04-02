#![deny(clippy::if_same_then_else)]

use async_openai::types::ChatCompletionRequestMessage;
use async_openai::types::CreateChatCompletionRequestArgs;
use async_openai::types::Role;
use futures::StreamExt;
use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::io::{stdout, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

async fn perform_request_with_streaming(
    home_dir: PathBuf,
    chat_history: Vec<ChatCompletionRequestMessage>,
) -> ChatCompletionRequestMessage {
    // For speech synthesis
    let mut token_buffer: Vec<char> = Vec::new();
    let mut buffer_sentences: VecDeque<String> = VecDeque::new();
    let mut process = Command::new("echo").stdout(Stdio::piped()).spawn().unwrap();

    // For code assistance
    let mut code_token_buffer: Vec<char> = Vec::new();
    let mut code_snippets: VecDeque<String> = VecDeque::new();

    // To save the current reply
    let mut current_reply: Vec<String> = Vec::new();

    // Set up the request
    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-3.5-turbo")
        .messages(chat_history)
        .stream(true)
        .build()
        .unwrap();

    let client = async_openai::Client::new();
    let mut response = client.chat().create_stream(request).await.unwrap();

    let mut lock = stdout().lock();

    print!("\nAssistant: ");
    while let Some(result) = response.next().await {
        let mut response = result.expect("Error while reading response");
        let something = response.choices.pop().unwrap();

        if let Some(ref token) = something.delta.content {
            write!(lock, "{}", token).unwrap();
            stdout().flush().unwrap();

            // Add the token to the current reply
            current_reply.push(token.clone());

            // Code assistance
            if code_token_buffer.is_empty() && token == "`" {
                if let Some(last_char) = token_buffer.last() {
                    if last_char == &'`' {
                        // If the last character in token buffer is a backtick,
                        // then it's a code block
                        token_buffer.pop();
                        code_token_buffer.push('`');
                        code_token_buffer.push('`');
                        continue;
                    }
                }
                // Temporarily append to main token buffer
                // Treat it as a normal text
            } else if code_token_buffer.is_empty() && token.starts_with('`') {
                // Empty code buffer
                code_token_buffer.extend(token.chars());
                continue;
            } else if code_token_buffer.len() == 2 && code_token_buffer[..2] == ['`', '`'] {
                // code_token_buffer has `` in it
                code_token_buffer.extend(token.chars());
                continue;
            } else if code_token_buffer.len() >= 3
                && code_token_buffer[..3] == ['`', '`', '`']
                && code_token_buffer[code_token_buffer.len() - 2..] == ['`', '`']
                && token.starts_with('`')
            {
                // code_token_buffer has should be flushed in it
                code_snippets.push_back(code_token_buffer.iter().collect());
                code_token_buffer.clear();
                continue;
            } else if !code_token_buffer.is_empty() {
                code_token_buffer.extend(token.chars());
                continue;
            }

            // Speech synthesis
            if token.starts_with(&['.', ':', '\n', '!', '?'][..])
                || token.ends_with(&['.', ':', '\n', '!', '?'][..])
            {
                // Concatenate, then say the sentence using macOS's say command
                let sentence = token_buffer.iter().collect::<String>();
                token_buffer.clear();

                buffer_sentences.push_back(sentence);

                if let Some(_exit_status) = process.try_wait().unwrap() {
                    if !buffer_sentences.is_empty() {
                        let say_sentence =
                            buffer_sentences.drain(..).collect::<Vec<String>>().join("");
                        process = Command::new("say")
                            .arg("-r")
                            .arg("200")
                            .arg("-v")
                            .arg("samantha")
                            .arg(&say_sentence)
                            .stdout(Stdio::null())
                            .spawn()
                            .unwrap();
                        buffer_sentences.clear();
                    }
                }
            } else {
                // If the token is not a period, append it to the buffer
                token_buffer.extend(token.chars());
            }
        }
    }

    // Flush any remaining buffer
    if !code_token_buffer.is_empty() {
        code_snippets.push_back(code_token_buffer.iter().collect());
        code_token_buffer.clear();
    }

    // If there are any code snippets, execute them first
    while let Some(code_snippet) = code_snippets.pop_front() {
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

        if language == "bash" {
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
        } else if let Some(filename) = filename {
            let filepath = home_dir.join(filename);
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
                    let filepath = home_dir.join(filename.trim());
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

    // If there are any sentences left in the buffer, say them
    while let Some(sentence) = buffer_sentences.pop_front() {
        process.wait().unwrap();
        process = Command::new("say")
            .arg("-r")
            .arg("200")
            .arg("-v")
            .arg("samantha")
            .arg(&sentence)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
    }

    // Append the current reply to the chat history and clear the current reply
    ChatCompletionRequestMessage {
        role: Role::Assistant,
        content: current_reply.join(""),
        name: None,
    }
}

async fn chat() {
    let args: Vec<String> = env::args().collect();

    let mut chat_history: Vec<_> = vec![ChatCompletionRequestMessage {
        role: Role::System,
        content: r#"You are going to be pair-programme with me. I need you to be less verbose in your explanations. 
    
        I need you to provide me at most one code block. 
            
        Please specify the language and the filename of the code block at the backticks
        
        ```<language>-<filename>
        <code> 
        ```.
        
            Most of the time we'll be working with one file at a time, represented by a code block. Changes to a code block should be rewritten entirely. Any suggestions or questions you have, please ask me. I'll be happy to answer them. Let's get started!"#.into(),
        name: None,
    }];

    let home_dir = Path::new(&args[1]);
    std::fs::create_dir_all(home_dir).expect("Could not create home directory");

    loop {
        println!("\nYou: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let prompt = ChatCompletionRequestMessage {
            role: Role::User,
            content: input.trim().to_string(),
            name: None,
        };
        chat_history.push(prompt);

        let reply =
            perform_request_with_streaming(home_dir.to_path_buf(), chat_history.clone()).await;

        chat_history.push(reply);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    chat().await;

    Ok(())
}
