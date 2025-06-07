use std::env;
use tokio;
use gemini_rust::{Gemini, Message, Role, Content};

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    // skip "koe" and join the rest
    if args.len() > 1 {
        let message: String = args[1..].join(" ");
        println!("{}", message);
    } else {
        println!("Please provide some text after 'koe'");
    }
}