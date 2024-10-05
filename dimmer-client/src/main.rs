mod communication_thead;

use serialport::{available_ports, SerialPort, SerialPortType};
use std::io::{self, Read, Write};
use std::str::FromStr;
use std::thread::spawn;
use std::time::{Duration, SystemTime};

use clap::{Arg, ArgMatches, Command};

use serialport::{DataBits, StopBits};
use crate::Error::{NoData, NumberFormat};
use std::sync::mpsc;
use std::thread;
use crc::{Crc, CRC_32_ISCSI};
use postcard::{from_bytes_crc32, to_slice_crc32};
use dimmer_communication::{CHANNELS_COUNT, ClientCommand, ClientCommandResult, GROUPS_COUNT};
use dimmer_communication::ClientCommandResultType::SetChannelEnabled;
use crate::communication_thead::{CommunicationThread, CommunicationThreadCommand};
use crate::communication_thead::CommunicationThreadCommand::{DimmerCommand, Stop};

struct PortData {
    port_name: String,
    baud_rate: u32,
    stop_bits: StopBits,
    data_bits: DataBits,
    refresh_rate: Duration,
    receive_timeout: Duration,
}

enum AppCommand {
    CommunicationCommand(CommunicationThreadCommand),
    Exit,
}

fn main() {
    let port_data = get_connect_data_from_arguments().unwrap();

    let port = serialport::new(&port_data.port_name, port_data.baud_rate)
        .timeout(Duration::from_millis(10))
        .open();

    match port {
        Ok(port) => {
            println!("Receiving data on {} at {} baud:", &port_data.port_name, &port_data.baud_rate);
            let (answer_sender, rx) = mpsc::channel();
            let(tx, command_source) = mpsc::channel();
            let mut communication_thread = CommunicationThread::new(port,
                command_source, answer_sender, Duration::from_millis(100));
            let communication_thread_instance = spawn(move || communication_thread.run());
            loop {
                let mut in_buff = String::new();
                io::stdin().read_line(&mut in_buff).expect("Error reading input!");
                let mut parts = in_buff.trim().split_whitespace();
                if let Some(command) = parts.next() {
                    match command {
                        "set_channel_enabled" => {
                            if let (Some(channel), Some(enabled)) = (parts.next(), parts.next()) {
                                let channel = parse_channel_or_show_error(channel.trim());
                                let enabled = parse_bool_or_show_error(enabled.trim());
                                if let (Some((channel, enabled))) = channel.zip(enabled) {
                                    println!(
                                        "Команда: {}, Канал: {}, Статус: {}",
                                        command, channel, enabled
                                    );
                                    let command = ClientCommand {
                                        id: 0,
                                        data: dimmer_communication::ClientCommandType::SetChannelEnabled { channel, enabled },
                                    };
                                    tx.send(DimmerCommand(command)).unwrap();
                                    println!("Команда відправлена");
                                }
                            } else {
                                println!("Невірний формат для команди 'set_channel_enabled'");
                            }
                        }
                        "exit" => {
                            tx.send(Stop).unwrap();
                            let stop_started = SystemTime::now();
                            while !communication_thread_instance.is_finished() {
                                if SystemTime::now().duration_since(stop_started).unwrap().as_secs() > 1 {
                                    break;
                                }
                                thread::sleep(Duration::from_millis(100));
                            }
                            break;
                        }
                        _ => {
                            println!("Unknown command: {}", command);
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to open \"{}\". Error: {}", port_data.port_name, e);
            ::std::process::exit(1);
        }
    }
    
    println!("Enter command")

}

fn parse_command(input: &str) -> Option<AppCommand> {
    let mut parts = input.trim().split_whitespace();
    if let Some(command) = parts.next() {
        match command {
            "set_channel_enabled" => {
                if let (Some(channel), Some(enabled)) = (parts.next(), parts.next()) {
                    let channel = parse_channel_or_show_error(channel.trim());
                    let enabled = parse_bool_or_show_error(enabled.trim());
                    if let (Some((channel, enabled))) = channel.zip(enabled) {
                        return Some(AppCommand::CommunicationCommand(DimmerCommand(
                            ClientCommand::set_channel_enabled(0, channel, enabled))));
                    }
                }
            }
            "exit" => {
                return Some(AppCommand::Exit);
            }
            _ => {
                println!("Unknown command: {}", command);
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

fn parse_channel_enabled_or_show_error(channel: &str, enabled: &str) -> Option<(u8, bool)> {
    let enabled: Option<bool> = enabled.parse()
        .inspect_err(
            |_| println!("enabled should be true or false! Entered: {}", enabled))
        .ok();
    let channel: Option<u8> = channel.parse()
        .inspect_err(
            |_| println!("channel should be number up to 22! Entered: {}", channel))
        .ok();
    channel.zip(enabled)

}


#[derive(Debug)]
enum Error {
    Io(serialport::Error),
    Clap(clap::Error),
    NoData(String), 
    NumberFormat(std::num::ParseIntError),
}

fn get_connect_data_from_arguments() ->Result<PortData, Error> {
    let ports = available_ports().map_err(Error::Io)?;
    let port_names: Vec<String> = ports.iter().map(|port| port.port_name.clone()).collect();
    println!("Available ports: {:?}", port_names);

    let matches = Command::new("Open serial port")
        .about("Enter data for serial port connection")
        .disable_version_flag(true)
        .arg(
            Arg::new("port")
                .help("The device path to a serial port")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::new("baud")
                .help("The baud rate to connect at")
                .takes_value(true)
                .possible_values(["4800", "9600", "19200", "38400", "57600", "115200", "230400", "460800", "921600"])
                .use_value_delimiter(false)
                .required(true),
        )
        .arg(
            Arg::new("stop-bits")
                .long("stop-bits")
                .help("Number of stop bits to use")
                .takes_value(true)
                .possible_values(["1", "2"])
                .default_value("1"),
        )
        .arg(
            Arg::new("data-bits")
                .long("data-bits")
                .help("Number of data bits to use")
                .takes_value(true)
                .possible_values(["5", "6", "7", "8"])
                .default_value("8"),
        )
        .arg(
            Arg::new("rate")
                .long("rate")
                .help("Frequency (Hz) to repeat reciving data")
                .takes_value(true)
                .validator(valid_number)
                .default_value("1"),
        )
        .arg(
            Arg::new("timeout")
                .long("receive data wait timeout")
                .help("Millis (ms) to wait data from rx")
                .takes_value(true)
                .default_value("1"),
        )
        .get_matches();

    let port_name = matches.value_of("port").ok_or(NoData("No port name provided!".to_string()))?;
    let baud_rate = matches.value_of("baud").ok_or(NoData("No baud rate provided!".to_string()))?
        .parse::<u32>().map_err(NumberFormat)?;
    let stop_bits = match matches.value_of("stop-bits") {
        Some("2") => StopBits::Two,
        _ => StopBits::One,
    };
    let data_bits = match matches.value_of("data-bits") {
        Some("5") => DataBits::Five,
        Some("6") => DataBits::Six,
        Some("7") => DataBits::Seven,
        _ => DataBits::Eight,
    };

    let refresh_rate_hz = matches.value_of("rate").ok_or(NoData("No refresh rate provided!".to_string()))?
        .parse::<u32>().map_err(NumberFormat)?;
    let refresh_rate = Duration::from_micros( (1000000 / refresh_rate_hz) as u64);

    let receive_timeout_ms = matches.value_of("timeout").ok_or(NoData("No receive timeout provided!".to_string()))?
        .parse::<u64>().map_err(NumberFormat)?;
    let receive_timeout = Duration::from_millis(receive_timeout_ms);

    Ok(PortData {
        port_name: port_name.to_string(),
        baud_rate,
        stop_bits,
        data_bits,
        refresh_rate,
        receive_timeout,
    })
}


fn valid_number(val: &str) -> Result<(), String> {
    val.parse::<u32>()
        .map(|_| ())
        .map_err(|_| format!("Invalid number '{}' specified", val))
}