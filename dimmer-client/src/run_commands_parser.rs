use std::time::Duration;
use clap::{Arg, Command};
use serialport::{available_ports, DataBits, StopBits};
use crate::errors::Error;
use crate::errors::Error::{NoData, NumberFormat};
use crate::PortData;

pub fn get_connect_data_from_arguments() ->Result<PortData, Error> {
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