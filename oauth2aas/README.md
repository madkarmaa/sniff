# oauth2aas

Exchanges a one-time Google OAuth token for an Android Authentication Service (AAS)
token, the long-lived credential Sniff logs in with.

## Usage

1. Open [Google's embedded setup page](https://accounts.google.com/EmbeddedSetup/identifier?flowName=EmbeddedSetupAndroid)
   with the browser's developer tools open, and log in with the Google account.
2. In the last requests of the login, find the `oauth_token` cookie. Its value starts
   with `oauth2_4/` and works only once.
3. Run the tool. It prints the AAS token.

    ```bash
    cargo run -p oauth2aas -- <email> <oauth_token>
    ```
