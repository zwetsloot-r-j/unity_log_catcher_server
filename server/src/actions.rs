extern crate warehouse;

use serde::{Serialize, Deserialize};
use std::sync::mpsc::Sender;
use crate::error::Error;
use crate::dto::LogDto;
use crate::state::State;
use warehouse::log::Log;

#[derive(Serialize, Deserialize, Debug)]
pub enum ExternalAction {
    Ping,
    StoreLog(LogDto),
    StoreLogs(Vec<LogDto>),
    ReadLogs(usize),
    RemoveClient,
    Recompile,
}

pub enum InternalAction {
    AddClient((usize, Sender<String>)),
}

pub enum Action {
    Internal(InternalAction),
    External((ExternalAction, usize, Sender<String>)),
}

impl ExternalAction {
    pub fn to_action(self, client_id: usize, sender: Sender<String>) -> Action {
        Action::External((self, client_id, sender))
    }

    #[allow(dead_code)] // used for debugging
    pub fn serialize(self) -> Result<String, Error> {
        serde_json::to_string(&self)
            .or(Error::SerializationFailed.as_result::<String>())
    }
}

impl Action {
    pub fn deserialize(source: &str, client_id : usize, sender: Sender<String>) -> Result<Action, Error> {
        println!("{}", source);

        if source == "Ping" {
            return Ok(ExternalAction::Ping.to_action(client_id, sender));
        }

        if source == "RemoveClient" {
            return Ok(ExternalAction::RemoveClient.to_action(client_id, sender));
        }

        if source == "Recompile" {
            return Ok(ExternalAction::Recompile.to_action(client_id, sender));
        }

        let external_action : ExternalAction = serde_json::from_str(source)
            .or(Error::DeserializationFailed.as_result::<ExternalAction>())?;
        Ok(external_action.to_action(client_id, sender))
    }
}

pub fn run(action: Action, mut state : State) -> Result<State, Error> {
    match action {
        Action::External((ExternalAction::Ping, _, sender)) => {
            println!("{}", "received ping message");

            sender.send("Pong".to_string())
                .or(Err(Error::FailedToSendResponse))
                .map(|_| state)
        },
        Action::Internal(InternalAction::AddClient((id, sender))) => {
            println!("{}", "client connected");
            state.clients.insert(id, sender);
            Ok(state)
        },
        Action::External((ExternalAction::RemoveClient, id, sender)) => {
            println!("{}", "client disconnected");
            state.clients.remove(&id);
            sender.send("Close".to_string())
                .or(Err(Error::FailedToSendResponse))
                .map(|_| state)
        },
        Action::External((ExternalAction::Recompile, _, _)) => {
            for (_, client) in state.clients.iter() {
                let result = client.send("Recompile".to_string());
                if result.is_err() {
                    println!("{}", "Failed to send command {Recompile} to a client");
                }
            }
            Ok(state)
        },
        Action::External((ExternalAction::StoreLog(log), _, _)) => {
            let conn = Log::connection()
                .or(Err(Error::FailedDbConnection))?;

            log.to_log()?
                .persist(&conn)
                .or(Err(Error::FailedToWriteToDb))?;

            conn.close()
                .or(Err(Error::FailedToCloseDbConnection))
                .map(|_| state)
        },
        Action::External((ExternalAction::StoreLogs(logs), _, _)) => {
            let conn = Log::connection()
                .or(Err(Error::FailedDbConnection))?;

            logs
                .into_iter()
                .map(|log| log.to_log()
                     .or(Err(Error::FailedToWriteToDb))?
                     .persist(&conn)
                     .or(Err(Error::FailedToWriteToDb))
                     .map(|_| ())
                 )
                .collect::<Result<Vec<()>, Error>>()
                .or(Err(Error::FailedToWriteToDb))?;

            conn.close()
                .or(Err(Error::FailedToCloseDbConnection))
                .map(|_| state)
        },
        Action::External((ExternalAction::ReadLogs(_max), _, sender)) => {
            let conn = Log::connection()
                .or(Err(Error::FailedDbConnection))?;

            let logs : Vec<LogDto> = Log::fetch(&conn)
                .or(Err(Error::FailedToReadFromDb))?
                .into_iter()
                .map(LogDto::from_log)
                .collect();

            let response = serde_json::to_string(&logs)
                .or(Err(Error::SerializationFailed))?;

            sender.send(response)
                .or(Err(Error::FailedToSendResponse))
                .map(|_| state)
        }
    }
}
