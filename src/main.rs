#![deny(clippy::if_same_then_else)]

mod code_assistant;
mod stt_assistant;
mod tts_assistant;

use async_openai::types::ChatCompletionRequestMessage;
use async_openai::types::CreateChatCompletionRequestArgs;
use async_openai::types::Role;
use code_assistant::CodeAssistant;
use futures::StreamExt;
use tts_assistant::TtsAssistant;

use std::env;
use std::error::Error;
use std::io::{stdout, Write};
use std::path::Path;

async fn perform_request_with_streaming(
    chat_history: Vec<ChatCompletionRequestMessage>,
    speech_assistant: &mut TtsAssistant,
    code_assistant: &mut CodeAssistant,
) -> ChatCompletionRequestMessage {
    // To save the current reply
    let mut current_reply: Vec<String> = Vec::new();

    // Set up the request
    let request = CreateChatCompletionRequestArgs::default()
        .model("gpt-3.5-turbo")
        .messages(chat_history)
        .stream(true)
        .build()
        .unwrap();

    // Make the request
    let client = async_openai::Client::new();
    let mut response = client.chat().create_stream(request).await.unwrap();

    // Acquite the stdout lock to print the assistant's response
    let mut lock = stdout().lock();

    let state = State::Prose;

    let mut code_buffer = vec![];
    let mut speech_buffer: Vec<char> = vec![];
    let mut tmp_buffer = vec![];

    // Process the stream
    while let Some(result) = response.next().await {
        let mut response = result.expect("Error while reading response");
        let something = response.choices.pop().unwrap();

        if let Some(ref token) = something.delta.content {
            // Display the token
            write!(lock, "{}", token).unwrap();
            stdout().flush().unwrap();

            // Add the token to the current reply
            current_reply.push(token.clone());

            // Process token according to state
            let (state, event) = transition(state, token, &code_buffer);
            match (state, event) {
                (State::Code, Event::Append) => {
                    code_assistant.push(&token.chars().collect::<Vec<_>>());
                    tmp_buffer.clear();
                }
                (State::Code, Event::Flush) => {
                    code_assistant.push(&token.chars().collect::<Vec<_>>());
                    code_assistant.flush();
                    code_buffer.clear();
                }
                (State::Code, _) => {}

                (State::Prose, Event::Append) => {
                    speech_assistant.push(&token.chars().collect::<Vec<_>>());
                    tmp_buffer.clear();
                }
                (State::Prose, Event::Flush) => {
                    speech_assistant.push(&token.chars().collect::<Vec<_>>());
                    speech_assistant.flush();
                    speech_buffer.clear();
                }
                (State::Prose, _) => {}

                (State::MaybeCode, Event::AppendTmp) => {
                    tmp_buffer.extend(token.chars());
                }
                _ => {}
            }
        }
    }

    // Flush any remaining buffer
    code_assistant.flush();
    speech_assistant.flush();

    // Append the current reply to the chat history and clear the current reply
    ChatCompletionRequestMessage {
        role: Role::Assistant,
        content: current_reply.join(""),
        name: None,
    }
}

async fn chat() {
    // Environment
    let args: Vec<String> = env::args().collect();
    let home_dir = Path::new(&args[1]).to_path_buf();

    // Initial intent
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

    // Assistants
    let mut speech_assistant = TtsAssistant::default();
    let mut code_assistant = CodeAssistant::new(home_dir);
    let mut stt =
        stt_assistant::Stt::new("/Users/raimibinkarim/Desktop/ggml-tiny.en.bin".to_string());

    // Turn-based
    loop {
        // User
        println!("\nYou: ");
        // let mut input = String::new();
        // std::io::stdin().read_line(&mut input).unwrap();
        // let text = input.trim().to_string()
        let text = stt.record();
        let prompt = ChatCompletionRequestMessage {
            role: Role::User,
            content: text,
            name: None,
        };
        chat_history.push(prompt);

        // Assistant
        print!("\nAssistant: ");
        let reply = perform_request_with_streaming(
            chat_history.clone(),
            &mut speech_assistant,
            &mut code_assistant,
        )
        .await;
        chat_history.push(reply);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    chat().await;

    Ok(())
}

fn transition(from: State, token: &str, code_buffer: &[char]) -> (State, Event) {
    match (from, token) {
        (State::Code, token) => {
            let token_backticks = token.chars().filter(|&x| x == '`').count();
            let num_backticks = code_buffer.iter().filter(|&x| x == &'`').count();
            if num_backticks + token_backticks == 6 {
                (State::Prose, Event::Flush)
            } else {
                (State::Code, Event::Append)
            }
        }

        (State::MaybeCode, "`" | "``") => (State::Code, Event::Append),
        (State::MaybeCode, _) => (State::Prose, Event::Append),

        (State::Prose, "`") => (State::MaybeCode, Event::AppendTmp),
        (State::Prose, "``" | "```") => (State::Code, Event::Append),
        (State::Prose, _) => (State::Prose, Event::Append),
    }
}

fn should_flush() {}

#[derive(PartialEq, Debug)]
enum Event {
    Flush,
    Append,
    AppendTmp,
}

#[derive(PartialEq, Debug, Clone, Copy)]
enum State {
    Code,
    MaybeCode,
    Prose,
}

// transition tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_code_transition() {
        assert_eq!(
            transition(State::Code, "`", &['`']),
            (State::Code, Event::Append)
        );
        assert_eq!(
            transition(State::Code, "``", &['`']),
            (State::Code, Event::Append)
        );
        assert_eq!(
            transition(
                State::Code,
                "`",
                "```code``".chars().collect::<Vec<_>>().as_slice()
            ),
            (State::Prose, Event::Flush)
        );
        assert_eq!(
            transition(
                State::Code,
                "``",
                "```code`".chars().collect::<Vec<_>>().as_slice()
            ),
            (State::Prose, Event::Flush)
        );
    }

    #[test]
    fn test_from_maybe_transition() {
        assert_eq!(
            transition(State::MaybeCode, "`", &['`']),
            (State::Code, Event::Append)
        );
        assert_eq!(
            transition(State::MaybeCode, "``", &['`']),
            (State::Code, Event::Append)
        );
        assert_eq!(
            transition(State::MaybeCode, "a", &['`']),
            (State::Prose, Event::Append)
        );
    }

    #[test]
    fn test_prose_transition() {
        assert_eq!(
            transition(State::Prose, "`", &[]),
            (State::MaybeCode, Event::AppendTmp)
        );
        assert_eq!(
            transition(State::Prose, "`", &['a']),
            (State::MaybeCode, Event::AppendTmp)
        );
    }
}
