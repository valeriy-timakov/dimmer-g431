use std::sync::Arc;
use tokio::sync::Mutex;
use futures::future::join_all;

// Тип обробника подій
type EventHandler = Box<dyn Fn() + Send + Sync>;

// Структура, що містить список обробників
pub struct EventManager {
    handlers: Arc<Mutex<Vec<EventHandler>>>,
}

impl EventManager {
    // Створення нового екземпляру
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    // Метод для додавання нового обробника
    pub async fn add_handler(&self, handler: EventHandler) {
        let mut handlers = self.handlers.lock().await;
        handlers.push(handler);
    }

    // Метод для запуску обробників події
    pub async fn trigger_event(&self) {
        // Отримуємо копію всіх обробників
        // let handlers = {
        //     let handlers = self.handlers.lock().await;
        //     handlers.clone()
        // };
        // 
        // // Запускаємо всі обробники асинхронно
        // let futures = handlers.iter().map(|handler| {
        //     tokio::spawn(async move {
        //         handler();
        //     })
        // });
        // 
        // // Чекаємо завершення всіх обробників
        // join_all(futures).await;
    }
}

async fn test() {
    let manager = EventManager::new();

    // Додаємо обробники з різних асинхронних контекстів
    // let manager_clone = manager.clone();
    // tokio::spawn(async move {
    //     manager_clone.add_handler(Box::new(|| {
    //         println!("Handler 1 executed!");
    //     }))
    //         .await;
    // });
    // 
    // let manager_clone = manager.clone();
    // tokio::spawn(async move {
    //     manager_clone.add_handler(Box::new(|| {
    //         println!("Handler 2 executed!");
    //     }))
    //         .await;
    // });

    // Чекаємо трохи часу, щоб обробники були додані
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Запускаємо подію, яка виконає всі обробники
    manager.trigger_event().await;
}
