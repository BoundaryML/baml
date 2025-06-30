use anyhow::Result;
use clap::Subcommand;
use console::style;

use crate::propelauth::PersistedTokenData;

#[derive(Subcommand, Debug)]
pub(crate) enum AuthCommands {
    #[command(about = "Login to Boundary Cloud")]
    Login(LoginArgs),

    #[command(about = "Generate an access token")]
    Token(TokenArgs),
}

impl AuthCommands {
    pub async fn run_async(&self) -> Result<()> {
        match self {
            AuthCommands::Login(args) => args.run_async().await,
            AuthCommands::Token(args) => args.run_async().await,
        }
    }
}

#[derive(clap::Args, Debug)]
pub struct LoginArgs {}

impl LoginArgs {
    pub async fn run_async(&self) -> Result<()> {
        let propel_auth_client = super::propelauth::PropelAuthClient::new()?;
        let mut token_data = propel_auth_client.run_authorization_code_flow().await?;

        let user_info = propel_auth_client
            .get_user_info(token_data.access_token().await?)
            .await?;
        token_data.write_to_storage()?;

        println!("{} Authentication successful!", style("✓").bold().green());
        println!(
            "{} Logged in as {}",
            style("✓").bold().green(),
            style(&user_info.email).bold(),
        );

        Ok(())
    }
}

#[derive(clap::Args, Debug)]
pub struct TokenArgs {}

impl TokenArgs {
    pub async fn run_async(&self) -> Result<()> {
        let mut token_data = PersistedTokenData::read_from_storage()?;
        let token = token_data.access_token().await?;
        println!("{token}");
        Ok(())
    }
}
