use std::env;

use gpapi::Gpapi;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args: Vec<String> = env::args().collect();

    let email = args
        .get(1)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing email"))?;
    let oauth2 = args
        .get(2)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing oauth2 token"))?;

    let mut api = Gpapi::new("px_9_fold", email.as_str());
    println!("{:?}", api.request_aas_token(oauth2.as_str()).await);
    let aas_token = api
        .get_aas_token()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "missing aas token"))?;
    println!("{aas_token:?}");
    Ok(())
}
