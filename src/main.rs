use std::env;
use tokio;
use gemini_rust::{Gemini};
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use std::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let gemini_api_key = match std::env::var("GEMINI_API_KEY") {
      Ok(key) => key,
      Err(_) => {
        return Err("GEMINI_API_KEY must be set".into());
      }
    };

    let model = "models/gemini-2.5-flash-preview-04-17".to_string();
    let gemini = Gemini::with_model(gemini_api_key, model);

    let system_prompt =
    format!("You are an interpreter of natural language to CLI commands.
    The user will input a task and you will output a CLI command that will execute the task.
    Do not explain the command, do not return any other text, simply return the command to be run in unix shell.
    ");

    // skip "koe" and join the rest
    if args.len() > 1 {
        let message = args[1..].join(" ");

        // Start loading spinner in a separate thread
        let (tx, rx) = mpsc::channel();
        let spinner_handle = thread::spawn(move || {
            let spinner_chars = vec!['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0;
            loop {
                // Check if we should stop
                if rx.try_recv().is_ok() {
                    break;
                }
                print!("\r{} Thinking... ", spinner_chars[i]);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(100));
                i = (i + 1) % spinner_chars.len();
            }
        });

        let response = gemini.generate_content()
            .with_system_prompt(system_prompt)
            .with_temperature(0.0) // No randomness
            .with_user_message(&message)
            .execute()
            .await?;

        // Stop the spinner thread
        tx.send(()).unwrap();
        spinner_handle.join().unwrap();
        print!("\r\x1B[K"); // Clear the spinner line
        println!("{}\nExecute command? (y/n)", response.text());

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        if input.trim() == "y" {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(response.text())
                .output()
                .unwrap();
            println!("{}", String::from_utf8_lossy(&output.stdout));
        } else {
            println!("Command not executed.");
        }
    } else {
        println!("Invalid usage. Enter a task after 'koe'");
    }

    Ok(())
}