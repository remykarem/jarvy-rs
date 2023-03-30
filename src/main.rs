#![deny(clippy::if_same_then_else)]

use async_openai::types::ChatCompletionRequestMessageArgs;
use async_openai::types::CreateChatCompletionRequestArgs;
use async_openai::types::Role;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::env;
use std::error::Error;
use std::io::{stdout, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ChatRequestBody {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: i32,
    stream: bool,
}

#[derive(Deserialize)]
struct ChatResponseDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponseChoice {
    delta: ChatResponseDelta,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatResponseChoice>,
}

async fn perform_request_with_streaming(
    home_dir: PathBuf,
    exit_code: i32,
    output: String,
    context: String,
    language: String,
) -> (i32, String) {
    // For speech synthesis
    let mut token_buffer: Vec<char> = Vec::new();
    let mut buffer_sentences: VecDeque<String> = VecDeque::new();
    let mut process = Command::new("echo").stdout(Stdio::piped()).spawn().unwrap();

    // For code assistance
    let mut code_token_buffer: Vec<char> = Vec::new();
    let mut code_snippets: VecDeque<String> = VecDeque::new();

    // To save the current reply and add to the chat history
    let mut current_reply = Vec::new();

    // Set up the request
    let url = "https://api.openai.com/v1/chat/completions";

    let prompt = if exit_code == 0 {
        let raw_prompt = if !context.is_empty() {
            println!("\nYou (with context): ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            format!("{}\n\n```{}\n{}\n```", input.trim(), language, context)
        } else {
            println!("\nYou: ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            input.trim().to_string()
        };
        ChatMessage {
            role: "user".to_string(),
            content: raw_prompt,
        }
    } else {
        ChatMessage {
            role: "user".to_string(),
            content: format!("I'm getting the following: ```\n{}\n```", output),
        }
    };

    let chat_history = vec![prompt];

    let mut headers = reqwest::header::HeaderMap::new();

    headers.insert(
        reqwest::header::ACCEPT,
        "text/event-stream".parse().unwrap(),
    );
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {}", API_KEY).parse().unwrap(),
    );

    let body = ChatRequestBody {
        model: "gpt-3.5-turbo".to_string(),
        messages: chat_history,
        temperature: 0,
        stream: true,
    };

    let mut response = Client::new()
        .post(url)
        .json(&body)
        .headers(headers)
        .send()
        .await
        .unwrap();

    print!("\nAssistant: ");
    while let Some(chunk) = response.chunk().await.unwrap() {
        let line = String::from_utf8_lossy(&chunk);

        let line = line.trim_start_matches("data: ");

        if line == "[DONE]" {
            break;
        }

        println!("yooo{}yooo", line);
        let ChatResponse { mut choices } = serde_json::from_str(line.trim()).unwrap();

        let ChatResponseChoice { delta } = choices.pop().unwrap();

        if let Some(token) = delta.content {
            print!("{}", token);

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
                if token.starts_with('`') {
                    code_token_buffer.extend(token.chars());
                } else {
                    for c in token.chars() {
                        code_token_buffer.push(c);
                    }
                }
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
                for c in token.chars() {
                    token_buffer.push(c);
                }
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
    let _chat_history = vec![ChatMessage {
        role: "assistant".to_string(),
        content: current_reply.join(""),
    }];
    // TODO: Add chat history to some persistent storage

    current_reply.clear();

    (exit_code, output)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // create client, reads OPENAI_API_KEY environment variable for API key.
    let client = async_openai::Client::new();

    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-3.5-turbo")
        .messages([ChatCompletionRequestMessageArgs::default()
            .content("What is pin and unpin in rust eli5")
            .role(Role::User)
            .build()?])
        .stream(true)
        .build()?;

    let mut stream = client.chat().create_stream(request).await?;

    let mut lock = stdout().lock();
    while let Some(result) = stream.next().await {
        match result {
            Ok(response) => {
                response.choices.iter().for_each(|chat_choice| {
                    if let Some(ref content) = chat_choice.delta.content {
                        write!(lock, "{}", content).unwrap();
                    }
                });
            }
            Err(err) => {
                writeln!(lock, "error: {err}").unwrap();
            }
        }
        stdout().flush()?;
    }

    Ok(())
}

async fn chat() {
    let args: Vec<String> = env::args().collect();

    let home_dir = Path::new(&args[1]);
    std::fs::create_dir_all(home_dir).expect("Could not create home directory");

    let (mut shared_file, mut context, mut language) = (None, String::new(), String::new());

    // If there are 2 args, the 2nd arg is the file
    if args.len() >= 3 {
        shared_file = Some(Path::new(&args[2]));
        if let Some(file) = shared_file {
            if !file.exists() {
                println!("File {} does not exist", file.display());
                std::process::exit(1);
            }
            context = std::fs::read_to_string(file).unwrap_or_default();
            language = args[3].clone();
        }
    }

    let mut exit_code = 0;
    let mut output = String::new();

    loop {
        match perform_request_with_streaming(
            home_dir.to_path_buf(),
            exit_code,
            output.clone(),
            context.clone(),
            language.clone(),
        )
        .await
        {
            (code, out) => {
                exit_code = code;
                output = out;
            }
            _ => {
                exit_code = 0;
                output = String::new();
            }
        }
    }
}
