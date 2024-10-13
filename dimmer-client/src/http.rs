use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use warp::Filter;

use dimmer_communication::{ClientCommand, PINS_COUNT};

use crate::{App, AppCommand, AppCommandResult};
use crate::communication_thead::CommunicationThreadCommand::DimmerCommand;
use crate::errors::Error;

pub fn spawn_http<ADDR: Into<SocketAddr> + Send + 'static>(addr: ADDR, app: Arc<Mutex<App>>) -> JoinHandle<()> {
    println!("try tu run HTTP server");
    tokio::task::spawn( run_http(addr, app) )
}


pub async fn run_http<ADDR: Into<SocketAddr>>(addr: ADDR, app: Arc<Mutex<App>>) {

    let tmp_app = app.clone();
    let get_value_route = warp::path!("board" / u8 / "pin" / u8)
        .and(warp::get())
        .map(handle_get_value)
        .and(warp::any().map(move || tmp_app.clone() ))
        .and_then(handle_command);


    let tmp_app = app.clone();
    let set_value_route = warp::path!("board" / u8 / "pin" / u8)
        .and(warp::post())
        .and(warp::body::json())
        .map(handle_set_value)
        .and(warp::any().map(move || tmp_app.clone() ))
        .and_then(handle_command);

    let routes = get_value_route.or(set_value_route);

    warp::serve(routes).run(addr).await
}



async fn handle_command(value: Result<AppCommand, ValidationError>, app: Arc<Mutex<App>>) -> Result<impl warp::Reply, Infallible> {
    match value {
        Ok(command) => {
            match app.lock().await.process_command(command).await {
                Ok(AppCommandResult::CommunicationCommandResult(result)) => {
                    Ok(warp::reply::json(&result))
                }
                Err(processing_error) => {
                    Ok(warp::reply::json(&ProcessingError::from(processing_error)))
                }
            }
        }
        Err(validation_error) => {
            Ok(warp::reply::json(&validation_error))
        }
    }
}

// GET /channel/value/{channel_no}
fn handle_get_value(board: u8, pin: u8) -> Result<AppCommand, ValidationError> {
    if pin > PINS_COUNT as u8 {
        return Err(ValidationError::PinNumberOverflow(pin));
    }
    Ok(AppCommand::CommunicationCommand(DimmerCommand(
        ClientCommand::get_pin_state(0, pin))))
}

// POST /channel/value/{channel_no}
fn handle_set_value(board: u8, pin: u8, on: bool) -> Result<AppCommand, ValidationError> {
    if pin > PINS_COUNT as u8 {
        return Err(ValidationError::PinNumberOverflow(pin));
    }
    Ok(AppCommand::CommunicationCommand(DimmerCommand(
        ClientCommand::set_pin_state(0, pin, on))))
}




#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
enum ValidationError {
    PinNumberOverflow(u8),
}


#[derive(Serialize, Deserialize, Debug, Clone)]
enum ProcessingError<'a> {
    Io(&'a str),
    Clap(&'a str),
    NoData(&'a str),
    NumberFormat(&'a str),
    PipelineSendError(&'a str),
    UndefinedError(&'a str),
}

impl<'a> From<Error> for ProcessingError<'a> {
    fn from(error: Error) -> Self {
        match error {
            Error::StdIo(_) => { return ProcessingError::Io("Some io error occurred!"); }
            Error::Io(_) => { return ProcessingError::Io("Some io error occurred!"); }
            Error::NoData(_) => { return ProcessingError::NoData("Some no data error occurred!"); }
            Error::NumberFormat(_) => { return ProcessingError::NumberFormat("Some number format error occurred!"); }
            Error::PipelineSendError(_) => { return ProcessingError::PipelineSendError("Some thread error occurred!"); }
            _ => { return ProcessingError::UndefinedError("Some unknown error occurred!"); }
        }
    }
}