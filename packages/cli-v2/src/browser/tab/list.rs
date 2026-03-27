use clap::Args;
use serde::{Deserialize, Serialize};

use crate::output::ResponseContext;

/// List tabs in a session
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct Cmd {
    /// Session ID
    #[arg(long)]
    pub session: String,
}

pub const COMMAND_NAME: &str = "browser.list-tabs";

pub fn context(cmd: &Cmd, _result: &crate::action_result::ActionResult) -> Option<ResponseContext> {
    Some(ResponseContext {
        session_id: cmd.session.clone(),
        tab_id: None,
        window_id: None,
        url: None,
        title: None,
    })
}
