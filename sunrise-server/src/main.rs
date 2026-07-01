use chrono::{Local, Timelike};
use clap::Parser;
use dropshot::{
    endpoint, ApiDescription, ConfigDropshot, ConfigLogging, ConfigLoggingLevel, HttpError,
    HttpResponseOk, HttpServerStarter, RequestContext,
};
use schemars::JsonSchema;
use serde::Serialize;
use std::net::SocketAddr;

/// Sunrise server configuration.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Config {
    /// Address to bind the HTTP server to.
    #[arg(long, env = "SUNRISE_BIND", default_value = "0.0.0.0:41399")]
    bind: SocketAddr,
}

#[derive(JsonSchema, Serialize)]
struct CommandResponse {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u8>,
}

fn current_command() -> CommandResponse {
    let now = Local::now();
    let total_minutes = now.hour() * 60 + now.minute();

    let window_start = 5 * 60 + 30; // 5:30 AM
    let window_end = 6 * 60 + 30; // 6:30 AM

    if (window_start..window_end).contains(&total_minutes) {
        let elapsed = total_minutes - window_start;
        let step = elapsed / 6; // 0..=9
        let value = (step * 10) as u8; // 0, 10, 20, ..., 90
        CommandResponse {
            command: "percent".into(),
            value: Some(value),
        }
    } else {
        CommandResponse {
            command: "reset".into(),
            value: None,
        }
    }
}

#[endpoint {
    method = GET,
    path = "/command",
}]
async fn get_command(
    _rqctx: RequestContext<()>,
) -> Result<HttpResponseOk<CommandResponse>, HttpError> {
    Ok(HttpResponseOk(current_command()))
}

fn api() -> ApiDescription<()> {
    let mut api = ApiDescription::new();
    api.register(get_command).unwrap();
    api
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let config = Config::parse();

    let config_dropshot = ConfigDropshot {
        bind_address: config.bind,
        ..Default::default()
    };

    let config_logging = ConfigLogging::StderrTerminal {
        level: ConfigLoggingLevel::Info,
    };

    let log = config_logging
        .to_logger("sunrise-server")
        .map_err(|e| e.to_string())?;

    let server = HttpServerStarter::new(&config_dropshot, api(), (), &log)
        .map_err(|e| e.to_string())?
        .start();

    server.await.map_err(|e| e.to_string())
}
