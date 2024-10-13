use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::{JoinHandle as StdJoinHandle, sleep, spawn};
use std::thread;
use std::time::{Duration, SystemTime};

use clap::{Arg, Command};
use serialport::{available_ports};
use serialport::{DataBits, StopBits};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Sender as TokioSender;
use dimmer_communication::ClientCommandResult;

use errors::Error::{NoData, NumberFormat};
use errors::Error;

use crate::cli::{CliSignal, spawn_cli};
use crate::communication_thead::{CommunicationThread, CommunicationThreadCommand, CommunicationThreadSignal};
use crate::communication_thead::CommunicationThreadCommand::Stop;
use crate::run_commands_parser::get_connect_data_from_arguments;

mod communication_thead;
mod cli;
mod errors;
mod run_commands_parser;
mod http;
mod event_manager;

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::Mutex;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio::task;
use tokio::task::JoinHandle;
use crate::event_manager::EventManager;
use crate::http::spawn_http;


const SYNC_CHANNELS_BUFFER_SIZE: usize = 100;



struct PortData {
    port_name: String,
    baud_rate: u32,
    stop_bits: StopBits,
    data_bits: DataBits,
    refresh_rate: Duration,
    receive_timeout: Duration,
}

#[derive(Debug)]
enum AppCommand {
    CommunicationCommand(CommunicationThreadCommand),
    Exit,
}

#[derive(Serialize, Deserialize, Debug)]
enum AppCommandResult {
    CommunicationCommandResult(CommunicationThreadSignal),
}

impl AppCommandResult {
    fn id(&self) -> Option<u32> {
        match self {
            AppCommandResult::CommunicationCommandResult(signal) => signal.id(),
        }
    }
}

struct AppCommandsHandler {
    communication_in_tx: Sender<CommunicationThreadCommand>,
    
}


//impl Send for ExitHandler {}



#[tokio::main]
async fn main() {

   
    
    
    env_logger::init();
    let port_data = get_connect_data_from_arguments().unwrap();
    
    let app = App::new();
    let app = Arc::new(Mutex::new(app));
    
    let tmp_app = app.clone();

            
            let (cli_out_tx, mut cli_out_rx) = 
                tokio::sync::mpsc::channel(SYNC_CHANNELS_BUFFER_SIZE);
            let (cli_in_tx, cli_in_rx) =
                tokio::sync::mpsc::channel(SYNC_CHANNELS_BUFFER_SIZE);
            
            let cli = spawn_cli(cli_out_tx, cli_in_rx);
            
            let http = spawn_http(([127, 0, 0, 1], 3030), app.clone());
            
            loop {
                if let (mut app) = app.lock().await {
                    app.try_recv();
                    match cli_out_rx.try_recv() {
                        Ok(command) => {
                            match app.process_command(command).await {
                                Ok(result) => {
                                    debug!("Answer: {:?}", result);
                                }
                                Err(e) => {
                                    error!("Error: {:?}", e);
                                }
                            }
                        }
                        Err(TryRecvError::Empty) => {},
                        Err(TryRecvError::Disconnected) => {
                            eprintln!("CLI disconnected!");
                        }
                    }
                    if app.is_exit_started() {
                        break;
                    }
                }
            }    
            cli.await; 
            http.await;
            app.lock().await.exit_handler();

}

struct CommunicationData {
    in_tx: Sender<CommunicationThreadCommand>,
    out_rx: Receiver<CommunicationThreadSignal>,
    thread_join: StdJoinHandle<()>,
}

struct  App {
    future_manager: FutureManager,
    dimmer: Option<CommunicationData>,
    exit_started: bool,
    
}


impl App {
    
    fn new() -> Self {
        Self {
            future_manager: FutureManager::new(),
            dimmer: None,
            exit_started: false,
        }
    }
    
    fn is_exit_started(&self) -> bool {
        self.exit_started
    }
    
    fn exit_handler(&self) {
        if let Some(dimmer) = self.dimmer.as_ref() {   
            debug!("Повторний запуск вимкнення...");
            match dimmer.in_tx.send(Stop) {
                Ok(()) => {
                    debug!("Вимкнення заплановано");
                }
                Err(e) => {
                    error!("Error sending exit signal to communication thread: {}. Maybe it is already shut down...", e);
                }
            }
            let stop_started = SystemTime::now();
            while !dimmer.thread_join.is_finished() {
                if SystemTime::now().duration_since(stop_started).unwrap().as_secs() > 10 {
                    break;
                }
                sleep(Duration::from_millis(100));
            }
            debug!("Вимкнення завершено - вихію з програми");
        }
    }
    
    fn connect_to_dimmer(&mut self, port_data: PortData) -> Result<(), Error> {

        let port = serialport::new(&port_data.port_name, port_data.baud_rate)
            .timeout(Duration::from_millis(10))
            .open();
        
        let port = port.map_err(Error::SerialPortError)?;

        debug!("Receiving data on {} at {} baud:", &port_data.port_name, &port_data.baud_rate);
        let (out_tx, out_rx) = mpsc::channel();
        let(in_tx, in_rx) = mpsc::channel();
        let mut thread = CommunicationThread::new(
            port, in_rx, out_tx, Duration::from_millis(100));
        let thread_join = spawn(move || thread.run());

        let communication_data = CommunicationData {
            in_tx,
            out_rx,
            thread_join,
        };
        
        if let Some(old_dimmer) = self.dimmer.take() {
            //ignore errors
            let _ = old_dimmer.in_tx.send(Stop);
        }
        
        self.dimmer = Some(communication_data);
        
        Ok(())
          
    }
    
    fn try_recv(&mut self) {
        if let Some(dimmer) = self.dimmer.as_ref() {
            match dimmer.out_rx.try_recv() {
                Ok(signal) => {
                    self.handle_command_result(Ok(AppCommandResult::CommunicationCommandResult(signal)));
                }
                Err(mpsc::TryRecvError::Empty) => {},
                Err(mpsc::TryRecvError::Disconnected) => {
                    error!("Communication thread disconnected!");
                }
            }
        }
    }
    
    async fn handle_command_result(&mut self, result: Result<AppCommandResult, Error>) {
        match result { 
            Ok(result) => {
                debug!("Answer: {:?}", result);
                if let Some(id) = result.id() {
                    debug!("Answer id: {:?}", id);
                    match self.future_manager.execute_future(id, result).await {
                        Ok(()) => {
                            debug!("Answer: handled");                            
                        }
                        Err(e) => {
                            error!("Error: {}", e);
                        }
                    }
                } 
            }
            Err(processing_error) => {
                error!("Error: {:?}", processing_error);
            }
        }
    }
    
    async fn process_command(&mut self, command: AppCommand) -> Result<AppCommandResult, Error> {
        // Створюємо новий ф'ючерс.
        let (id, receiver) = self.future_manager.create_future().await;
        let result = match command {
            AppCommand::CommunicationCommand(mut command) => {
                command.set_id(id);
                debug!("Команда: {:?}", command);
                match self.dimmer.as_ref() { 
                    Some(dimmer) => {
                        dimmer.in_tx.send(command).unwrap();
                        debug!("Команда відправлена");
                        Ok(())
                    }
                    None => {
                        error!("Помилка відправки команди - немає з'єднання!");
                        Err(Error::NoSerialConnection)
                    }
                }
            }
            AppCommand::Exit => {
                if let Some(dimmer) = self.dimmer.as_ref() {
                    debug!("Старт вимкнення...");
                    match dimmer.in_tx.send(Stop) {
                        Ok(()) => {
                            debug!("Вимкнення завершено - вихід з програми");
                        }
                        Err(e) => {
                            error!("Error sending exit signal to communication thread: {}. Maybe it is already shut down...", e);
                        }
                    }
                    self.exit_started = true;
                }
                Ok(())
            }
        };
        
        match  result { 
            Ok(()) => {
                receiver.await.map_err(Error::RecvError)
            }
            Err(e) => {
                Err(e)
            }
        }
    }
}

async fn exit_app(
    communication_in_tx: &Sender<CommunicationThreadCommand>, 
    communication_thread_instance: StdJoinHandle<()>, 
    cli_in_tx: TokioSender<CliSignal>, 
   // cli: impl Future<Output=TokioJoinHandle<Result<(), Error>>> + Sized
) -> Result<(), Error> {
    println!("Старт вимкнення...");
    communication_in_tx.send(Stop).unwrap();
    if let Err(e) = cli_in_tx.send(CliSignal::Exit).await {
        eprintln!("Error sending exit signal to CLI: {}. Maybe it is already shut down...", e);
    }
    let stop_started = SystemTime::now();
    while !communication_thread_instance.is_finished() {
        if SystemTime::now().duration_since(stop_started).unwrap().as_secs() > 10 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
   // cli.await;
    println!("Вимкнення завершено - вихію з програми");
    Ok(())
}


// Структура, яка представляє ф'ючерс та приймає значення.
struct ManagedFuture {
    // Канал для передачі даних у ф'ючерс.
    sender: Option<oneshot::Sender<AppCommandResult>>,
}

impl ManagedFuture {
    // Створюємо новий ф'ючерс та повертаємо структуру з каналом.
    fn new() -> (Self, oneshot::Receiver<AppCommandResult>) {
        let (sender, receiver) = oneshot::channel();
        (Self { sender: Some(sender) }, receiver)
    }

    // Метод для виконання ф'ючерса, який приймає значення та передає його у ф'ючерс через канал.
    fn complete(&mut self, result: AppCommandResult) {
        if let Some(sender) = self.sender.take() {
            // Передаємо значення у ф'ючерс.
            let _ = sender.send(result);
        }
    }
}

struct FutureManager {
    // Мапа для зберігання ф'ючерсів за їх ID.
    futures: Arc<Mutex<HashMap<u32, ManagedFuture>>>,
    // Лічильник для генерації унікальних ID.
    id_counter: Arc<Mutex<u32>>,
}

impl FutureManager {
    // Конструктор, який ініціалізує мапу та лічильник.
    fn new() -> Self {
        Self {
            futures: Arc::new(Mutex::new(HashMap::new())),
            id_counter: Arc::new(Mutex::new(0)),
        }
    }

    // Метод для створення нового ф'ючерсу, додавання його до мапи та повернення ID та каналу.
    async  fn create_future(&self) -> (u32, oneshot::Receiver<AppCommandResult>) {
        // Генеруємо новий унікальний ID.
        let mut id_counter = self.id_counter.lock().await;
        *id_counter += 1;
        let id = *id_counter;

        // Створюємо новий ф'ючерс.
        let (managed_future, receiver) = ManagedFuture::new();

        // Додаємо ф'ючерс у мапу.
        let mut futures = self.futures.lock().await;
        futures.insert(id, managed_future);

        // Повертаємо ID ф'ючерсу та канал для прийому значень.
        (id, receiver)
    }

    // Метод для виконання ф'ючерсу за його ID та передавання значення у цей ф'ючерс.
    async fn execute_future(&self, id: u32, value: AppCommandResult) -> Result<(), String> {
        let mut futures = self.futures.lock().await;

        // Знаходимо ф'ючерс за ID.
        if let Some(future) = futures.get_mut(&id) {
            // Виконуємо ф'ючерс з переданим значенням.
            future.complete(value);
            Ok(())
        } else {
            Err(format!("Ф'ючерс з ID {} не знайдено.", id))
        }
    }
}

