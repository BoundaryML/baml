use anyhow::{Context, Result};
use axum::{extract::Path, extract::Query, routing::get, Router};
use base64::{engine::general_purpose, Engine as _};
use clap::{Parser, Subcommand};
use console::style;
use log::kv;
use reqwest;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use web_time::SystemTime;

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
        token_data.write_to_storage()?;
        println!("{} Authentication successful!", style("✓").bold().green());

        let user_info = propel_auth_client
            .get_user_info(token_data.access_token().await?)
            .await?;
        println!(
            "{} Logged in as {}",
            style("✓").bold().green(),
            style(user_info.email).bold(),
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
        println!("{}", token);
        Ok(())
    }
}
