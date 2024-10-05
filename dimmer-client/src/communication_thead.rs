use std::io;
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};
use crc::{Crc, CRC_32_ISCSI};
use postcard::{from_bytes_crc32, to_slice_crc32};
use serialport::SerialPort;
use dimmer_communication::{ClientCommand, ClientCommandResult};
use dimmer_communication::ClientCommandResultType::SetChannelEnabled;
use crate::communication_thead::CommunicationThreadSignal::{DimmerCommandAnswer};

pub enum CommunicationThreadCommand {
    Stop,
    DimmerCommand(ClientCommand),
    GetRunStatistics,
}

pub enum CommunicationThreadSignal {
    Stop,
    DimmerCommandAnswer(ClientCommandResult),
    RunStatistics(RunStatistics),
}

#[derive(Debug, Clone)]
pub struct RunStatistics {
    pub bytes_readed: usize,
    pub last_read_time_stamp: Option<SystemTime>,
    pub cycles_count: u64,
    pub started_at: Option<SystemTime>,
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
    run_statistics: RunStatistics, 
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
            run_statistics: RunStatistics {
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
                    CommunicationThreadCommand::GetRunStatistics => {
                        self.answer_sender.send(CommunicationThreadSignal::RunStatistics(
                            self.run_statistics.clone())).unwrap();
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
                println!("Read {} bytes: {:?}", bytes_readed, &self.serial_buf[self.bytes_readed..self.bytes_readed + bytes_readed]);
                self.bytes_readed += bytes_readed;
                self.run_statistics.bytes_readed += bytes_readed;
                self.last_read_time_stamp = SystemTime::now();
                self.busy = true;
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
            Err(e) => eprintln!("{:?}", e),
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
                println!("Received command: {:?}", dimmer_answer);
                match self.answer_sender.send(DimmerCommandAnswer(dimmer_answer)) {
                    Ok(_) => println!("Anser returned."),
                    Err(e) => eprintln!("Return anser error: {:?}", e),
                }
            }
            Err(e) => eprintln!("{:?}", e),
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
                eprintln!("Command source disconnected!");
                None
            }
        }
    }

    fn send_command(&mut self, command: ClientCommand) {
        match to_slice_crc32(&command, &mut self.command_send_buf, self.crc.digest()) {
            Ok(buff_slice) => {
                println!("sending: {:?}", buff_slice);
                self.port.write_all(buff_slice).unwrap();

                if let ClientCommand{data: dimmer_communication::ClientCommandType::SetChannelEnabled { channel, enabled }, ..} = command {
                    let expected = ClientCommandResult {
                        id: 0,
                        data: Ok(SetChannelEnabled { channel, enabled }),
                    };
                    match to_slice_crc32(&expected, &mut self.command_send_buf, self.crc.digest()) {
                        Ok(buff_slice) => {
                            println!(" answer should be: {:?}", buff_slice);
                        }
                        Err(e) => eprintln!("{:?}", e),
                    }
                }
            }
            Err(e) => eprintln!("{:?}", e),
        }
    }
}
