#![deny(clippy::if_same_then_else)]

mod code_assistant;
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

            // Do assistant tings
            let last_speech_ch = speech_assistant.last_char_in_buffer();
            code_assistant.process_token(token, last_speech_ch);
            speech_assistant.process_token(token);
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

    // Turn-based
    loop {
        // User
        println!("\nYou: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let prompt = ChatCompletionRequestMessage {
            role: Role::User,
            content: input.trim().to_string(),
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
