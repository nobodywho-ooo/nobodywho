use std::io::{Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // config
    let args: Vec<String> = std::env::args().collect();
    let model_path = args[1].clone();
    let use_gpu = true;

    // lets go
    let model = nobodywho::llm::get_model(&model_path, use_gpu)?;
    let chat = nobodywho::chat::ChatBuilder::new(model).build();

    // let response = chat.say_complete("Hi, who are you?").await?;
    // println!("{}", response);

    let mut token_out_count = 0;
    let mut stream = chat.say_stream("Tell me a story");
    while let Some(token) = stream.next_token().await {
        token_out_count += 1;
        print!("{}", token);
        std::io::stdout().flush().expect("failed flushing stdout");
    }
    Ok(())
}


