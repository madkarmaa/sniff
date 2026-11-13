use std::env;
use std::process::ExitCode;

use gpapi::Gpapi;

const USAGE: &str = "usage: oauth2aas <email> <oauth_token>";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let (Some(email), Some(oauth2)) = (args.get(1), args.get(2)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    match run(email, oauth2).await {
        Ok(aas_token) => {
            println!("{aas_token}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run(
    email: &str,
    oauth2: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut api = Gpapi::new("px_9_fold", email)?;
    api.request_aas_token(oauth2).await?;
    let aas_token = api
        .get_aas_token()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing aas token"))?;
    Ok(aas_token.to_owned())
}
