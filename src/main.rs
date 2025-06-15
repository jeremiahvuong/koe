use clap::Parser;
use gemini_rust::Gemini;
use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tokio;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Skips the execution prompt and runs the command automatically
    #[arg(short, long, default_value_t = false)]
    r#unsafe: bool,

    /// Task to be converted into a CLI command
    #[arg(num_args = 1..)]
    task_parts: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let gemini_api_key = match std::env::var("GEMINI_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            return Err("GEMINI_API_KEY must be set".into());
        }
    };

    let model = "models/gemini-2.5-flash-preview-05-20".to_string();
    let gemini = Gemini::with_model(gemini_api_key, model);

    let system_prompt =
        format!("You are an interpreter of natural language to CLI commands. \n
                The user will input a task and you will output a CLI command that will execute the task. \n
                Do not explain the command, do not return any other text, simply return the command to be run in the unix shell. \n
                Only return the command if you are 100% sure that the commands matches the task and runs successfully; if you are not sure, return 'unknown'. \n
                The user is currently running OS: {}. \n
                The user is currently in directory: {}.
                ",
                std::env::consts::OS,
                std::env::current_dir().unwrap().display()
            );

    let task = args.task_parts.join(" ");

    if task.is_empty() {
        println!("Invalid usage. Enter a task after 'koe' or use 'koe --help' for options.");
        return Ok(());
    }

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

    let response = gemini
        .generate_content()
        .with_system_prompt(system_prompt)
        .with_temperature(0.0) // No randomness
        // Few-shot prompting
        .with_user_message("create a new rust project on the desktop")
        .with_model_message("cd ~/desktop && cargo new my_project")
        .with_user_message("speedtest")
        .with_model_message("unknown")
        .with_user_message(&task)
        .execute()
        .await?;

    // Stop the spinner thread
    tx.send(()).unwrap();
    spinner_handle.join().unwrap();
    print!("\r\x1B[K"); // Clear the spinner line

    if response.text() == "unknown" {
        println!("Koe failed to understand the task. Please try again.");
        return Ok(());
    }

    let command_to_execute = response.text();

    let run_command_and_print_output = |command: &str| {
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
    };

    if args.r#unsafe {
        run_command_and_print_output(&command_to_execute);
    } else {
        loop {
            println!("{}\nExecute command? (y/n)", command_to_execute);

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();

            match input.trim().to_lowercase().as_str() {
                "y" => {
                    run_command_and_print_output(&command_to_execute);
                    break;
                }
                "n" => {
                    println!("Command not executed.");
                    break;
                }
                _ => {
                    println!("Invalid input. Please enter 'y' or 'n'.");
                }
            }
        }
    }

    Ok(())
}
