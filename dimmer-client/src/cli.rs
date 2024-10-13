use std::io;
use std::io::Write;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use dimmer_communication::{CHANNELS_COUNT, ClientCommand, GROUPS_COUNT, PINS_COUNT};

use crate::AppCommand;
use crate::communication_thead::CommunicationThreadCommand::DimmerCommand;
use crate::errors::Error;

pub enum CliSignal {
    Exit,
}

pub fn spawn_cli(input_sender: mpsc::Sender<AppCommand>, signals_receiver: mpsc::Receiver<CliSignal>) -> JoinHandle<Result<(), Error>> {
    println!("try tu run CLI");
    tokio::task::spawn(  run_command_line_interface(input_sender, signals_receiver) ) 
}

async fn run_command_line_interface(input_sender: mpsc::Sender<AppCommand>, mut signals_receiver: mpsc::Receiver<CliSignal>) -> Result<(), Error> {
    println!("Інтерактивний CLI запущено. Введіть 'exit' для завершення.");
    let mut input = String::new();

    loop {
        print!("> ");
        io::stdout().flush()
            .map_err(Error::StdIo)?;
        input.clear();
        
        if let Ok(signal) = signals_receiver.try_recv() {
            match signal {
                CliSignal::Exit => {
                    println!("Завершення CLI...");
                    return Ok(());
                }
            }
        }

        io::stdin()
            .read_line(&mut input)
            .map_err(Error::StdIo)?;

        let trimmed_input = input.trim();

        if trimmed_input.eq_ignore_ascii_case("exit") {
            println!("Завершення CLI...");
            input_sender.send(AppCommand::Exit)
                .await
                .map_err(|e| Error::PipelineSendError(e.0))?;
            return Ok(());
        }

        match parse_command(trimmed_input)
            .map(|command| input_sender.send(command)) {
            None => None, 
            Some(f) => Some(f.await),
        }
        .transpose()
        .map_err(|e| Error::PipelineSendError(e.0))?;

    }
}




fn parse_command(input: &str) -> Option<AppCommand> {
    let mut parts = input.split_whitespace();
    if let Some(command) = parts.next() {
        match command {
            "set_channel_enabled" => {
                match (parts.next(), parts.next()) {
                    (Some(channel), Some(enabled)) => {
                        let channel = parse_channel_or_show_error(channel.trim());
                        let enabled = parse_bool_or_show_error(enabled.trim());
                        if let Some((channel, enabled)) = channel.zip(enabled) {
                            return Some(AppCommand::CommunicationCommand(DimmerCommand(
                                ClientCommand::set_channel_enabled(0, channel, enabled))));
                        }
                    }
                    _ => println!("Не вистачає аргументів для команди set_channel_enabled!"),

                }
            }
            "pin_state" => {
                match (parts.next(), parts.next()) {
                    (Some(pin), Some(state)) => {
                        let pin = parse_channel_or_show_error(pin.trim());
                        let state = parse_bool_or_show_error(state.trim());
                        if let Some((pin, state)) = pin.zip(state) {
                            return Some(AppCommand::CommunicationCommand(DimmerCommand(
                                ClientCommand::set_pin_state(0, pin, state))));
                        }
                    }
                    _ => println!("Не вистачає аргументів для команди pin_state!"),

                }
            }
            _ => {
                println!("Невідома команда: {}", command);
            }
        }
    }
    None
}

fn parse_bool_or_show_error(value: &str) -> Option<bool> {
    value.parse()
        .inspect_err(
            |_| println!("Value should be true or false! Entered: {}", value))
        .ok()
}

fn parse_duty_or_show_error(value: &str) -> Option<f32> {
    value.parse()
        .inspect_err(
            |_| println!("Value should be a valid duty - decimal number from 0.0 to 1.0! Entered: {}", value))
        .ok()
}

fn parse_pin_or_show_error(value: &str) -> Option<u8> {
    value.parse()
        .inspect_err(
            |_| println!("Value should be number of pin - from 0 up to {}! Entered: {}", PINS_COUNT - 1, value))
        .ok()
}

fn parse_channel_or_show_error(value: &str) -> Option<u8> {
    value.parse()
        .inspect_err(
            |_| println!("Value should be number of channel - from 0 up to {}! Entered: {}", CHANNELS_COUNT - 1, value))
        .ok()
}

fn parse_channels_group_or_show_error(value: &str) -> Option<u8> {
    value.parse()
        .inspect_err(
            |_| println!("Value should be number of channels group - from 0 up to {}! Entered: {}", GROUPS_COUNT - 1, value))
        .ok()
}

