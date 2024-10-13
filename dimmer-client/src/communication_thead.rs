use std::io;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};
use crc::{Crc, CRC_32_ISCSI};
use postcard::{from_bytes_crc32, to_slice_crc32};
use serialport::SerialPort;
use dimmer_communication::{ClientCommand, ClientCommandResult};
use log::{  debug, error};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum CommunicationThreadCommand {
    Stop,
    DimmerCommand(ClientCommand),
    GetRunStatistics(u32),
}

impl CommunicationThreadCommand {
    pub fn set_id(&mut self, id: u32) {
        match self {
            CommunicationThreadCommand::DimmerCommand(command) => {
                command.id = id;
            }
            CommunicationThreadCommand::GetRunStatistics(_id) => {
                *_id = id;
            }
            _ => {}
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum CommunicationThreadSignal {
    DimmerCommandAnswer(ClientCommandResult),
    RunStatistics(RunStatistics),
}

impl CommunicationThreadSignal {
    pub fn id(&self) -> Option<u32> {
        match self {
            CommunicationThreadSignal::DimmerCommandAnswer(answer) => Some(answer.id),
            CommunicationThreadSignal::RunStatistics(statistics) => Some(statistics.id),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub struct RunStatisticsData {
    pub bytes_readed: usize,
    pub last_read_time_stamp: Option<SystemTime>,
    pub cycles_count: u64,
    pub started_at: Option<SystemTime>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub struct RunStatistics {
    pub id: u32,
    pub bytes_readed: usize,
    pub last_read_time_stamp: Option<SystemTime>,
    pub cycles_count: u64,
    pub started_at: Option<SystemTime>,
}

impl RunStatisticsData {
    fn to_answer(&self, id: u32) -> RunStatistics {
        RunStatistics {
            id,
            bytes_readed: self.bytes_readed,
            last_read_time_stamp: self.last_read_time_stamp,
            cycles_count: self.cycles_count,
            started_at: self.started_at,
        }
    }
}

pub struct CommunicationThread {
    port: Box<dyn SerialPort>,
    command_source: mpsc::Receiver<CommunicationThreadCommand>,
    answer_sender: mpsc::Sender<CommunicationThreadSignal>,
    serial_buf: Vec<u8>,
    crc: Crc::<u32>,
    bytes_readed: usize,
    last_read_time_stamp: SystemTime,
    read_wait_timeout: Duration,
    command_send_buf: Vec<u8>,
    busy: bool,
    run_statistics: RunStatisticsData, 
}

impl CommunicationThread {
    const BUFFER_SIZE: usize = 1000;

    pub fn new(
        port: Box<dyn SerialPort>,
        command_source: mpsc::Receiver<CommunicationThreadCommand>,
        answer_sender: mpsc::Sender<CommunicationThreadSignal>,
        read_wait_timeout: Duration
    ) -> Self {
        CommunicationThread {
            port,
            command_source,
            answer_sender,
            serial_buf: vec![0; Self::BUFFER_SIZE],
            crc: Crc::<u32>::new(&CRC_32_ISCSI),
            bytes_readed: 0,
            last_read_time_stamp: SystemTime::now(),
            read_wait_timeout,
            command_send_buf: vec![0; Self::BUFFER_SIZE],
            busy: false,
            run_statistics: RunStatisticsData {
                bytes_readed: 0,
                last_read_time_stamp: None,
                cycles_count: 0,
                started_at: None,
            },
        }
    }

    pub fn run(&mut self) {
        self.run_statistics.started_at = Some(SystemTime::now());
        loop {
            self.run_statistics.cycles_count += 1;
            self.try_read();
            if self.data_ready() {
                self.parse_and_return_answer();
            }
            if let Some(command) = self.next_command() {
                match command { 
                    CommunicationThreadCommand::Stop => {
                        return;
                    }
                    CommunicationThreadCommand::DimmerCommand(command) => {
                        self.send_command(command);
                    }
                    CommunicationThreadCommand::GetRunStatistics(id) => {
                        self.answer_sender.send(CommunicationThreadSignal::RunStatistics(
                            self.run_statistics.to_answer(id))).unwrap();
                    }
                }
            }
        }
    }

    fn try_read(&mut self) {
        match self.port.read(&mut self.serial_buf.as_mut_slice()[self.bytes_readed..]) {
            Ok(bytes_readed) => {
                if bytes_readed == 0 {
                    self.busy = false;
                    return;
                }
                debug!("Read {} bytes: {:?}", bytes_readed, &self.serial_buf[self.bytes_readed..self.bytes_readed + bytes_readed]);
                self.bytes_readed += bytes_readed;
                self.run_statistics.bytes_readed += bytes_readed;
                self.last_read_time_stamp = SystemTime::now();
                self.busy = true;
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => {
                self.busy = false;
            },
            Err(e) => error!("{:?}", e),
        }
    }

    fn data_ready(&self) -> bool {
        self.bytes_readed > 0 &&
            SystemTime::now().duration_since(self.last_read_time_stamp).unwrap() > self.read_wait_timeout
    }

    fn parse_and_return_answer(&mut self) {
        match from_bytes_crc32(&self.serial_buf.as_mut_slice()[0..self.bytes_readed], self.crc.digest()) {
            Ok(dimmer_answer) => {
                let dimmer_answer: ClientCommandResult = dimmer_answer;
                match self.answer_sender.send(CommunicationThreadSignal::DimmerCommandAnswer(dimmer_answer)) {
                    Ok(_) => debug!("Anser returned."),
                    Err(e) => error!("Return anser error: {:?}", e),
                }
            }
            Err(e) => error!("{:?}", e),
        }
        self.bytes_readed = 0;
    }

    fn next_command(&mut self) -> Option<CommunicationThreadCommand> {
        if self.busy {
            return None;
        }
        match self.command_source.try_recv() {
            Ok(command) => Some(command),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                error!("Command source disconnected!");
                None
            }
        }
    }

    fn send_command(&mut self, command: ClientCommand) {
        match to_slice_crc32(&command, &mut self.command_send_buf, self.crc.digest()) {
            Ok(buff_slice) => {
                debug!("sending: {:?}", buff_slice);
                self.port.write_all(buff_slice).unwrap();
            }
            Err(e) => error!("{:?}", e),
        }
    }
}
