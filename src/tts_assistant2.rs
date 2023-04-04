use std::{collections::VecDeque, io::Cursor, env};

use reqwest::Client;
use rodio::{Decoder, OutputStream, Sink};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct VoiceSettings {
    stability: u32,
    similarity_boost: u32,
}

#[derive(Serialize, Deserialize)]
struct TextToSpeechRequest<'a> {
    text: &'a str,
    voice_settings: VoiceSettings,
}

#[derive(Default)]
pub struct TtsAssistant2 {
    sentence_buffer: VecDeque<String>,
    is_running: bool,
}

fn play_audio(audio_data: Vec<u8>) {
    // assume you have your audio data in a Vec<u8> called `audio_data`
    let cursor = Cursor::new(audio_data);
    let source = Decoder::new(cursor).unwrap();

    // Open the default audio output device
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();

    // Play the audio
    sink.append(source);
    sink.sleep_until_end();
}

impl TtsAssistant2 {
    async fn say(&mut self, sentence: &str) {
        let api_url = "https://api.elevenlabs.io/v1/text-to-speech/";
        let voice_id = "EXAVITQu4vr4xnSDxMaL";
        let api_key = env::var("ELEVENLABS_API_KEY").expect("ELEVENLABS_API_KEY must be set");

        let client = Client::new();

        let voice_settings = VoiceSettings {
            stability: 0,
            similarity_boost: 0,
        };
        let request_body = TextToSpeechRequest {
            text: sentence,
            voice_settings,
        };
        let response = client
            .post(&format!("{}{}", api_url, voice_id))
            .header("xi-api-key", api_key)
            .json(&request_body)
            .send()
            .await.unwrap();

            if response.status().is_success() {

        let audio_data = response.bytes().await.unwrap().to_vec();

        // Play the audio
        play_audio(audio_data);

    } else {
        println!("Error: {}", response.status());
    }
    }
    pub async fn flush(&mut self) {
        let sentences = self
            .sentence_buffer
            .drain(..)
            .collect::<Vec<String>>()
            .join("");
        self.say(&sentences).await;
    }
    pub fn push(&mut self, chars: &[char]) {
        self.sentence_buffer.push_back(chars.iter().collect());
    }
}
