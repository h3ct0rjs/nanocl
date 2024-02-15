use std::{sync::Arc, collections::HashMap, time::Duration};

use ntex::{rt, time};
use futures_util::{Future, lock::Mutex};

use nanocl_error::io::{IoError, IoResult};

use nanocl_stubs::system::NativeEventAction;

use crate::tasks::generic::ObjTaskFuture;

#[derive(Clone)]
pub struct ObjTask {
  pub kind: NativeEventAction,
  pub fut: Arc<rt::JoinHandle<IoResult<()>>>,
}

impl ObjTask {
  pub fn new<F>(kind: NativeEventAction, task: F) -> Self
  where
    F: Future<Output = IoResult<()>> + 'static,
  {
    let fut = Arc::new(rt::spawn(task));
    Self { kind, fut }
  }

  pub async fn wait(&self) {
    loop {
      if self.fut.is_finished() {
        break;
      }
      time::sleep(Duration::from_secs(1)).await;
    }
  }
}

#[derive(Clone, Default)]
pub struct TaskManager {
  pub tasks: Arc<Mutex<HashMap<String, ObjTask>>>,
}

impl TaskManager {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn add_task(
    &self,
    key: &str,
    kind: NativeEventAction,
    task: ObjTaskFuture,
  ) {
    let key = key.to_owned();
    let key_ptr = key.clone();
    let tasks = self.tasks.clone();
    log::debug!("Creating task: {key} {}", kind);
    let new_task = ObjTask::new(kind.clone(), async move {
      if let Err(err) = task.await {
        log::error!("Task failed: {kind} {key_ptr} {}", err);
        return Err(err);
      };
      log::debug!("Task completed: {kind} {key_ptr}");
      tasks.lock().await.remove(&key_ptr);
      Ok::<_, IoError>(())
    });
    self.tasks.lock().await.insert(key, new_task.clone());
  }

  pub async fn remove_task(&self, key: &str) {
    let key = key.to_owned();
    let mut tasks = self.tasks.lock().await;
    let task = tasks.get(&key);
    if let Some(task) = task {
      task.fut.abort();
      log::debug!("Removing task: {key} {}", task.kind);
      tasks.remove(&key);
    }
  }

  pub async fn get_task(&self, key: &str) -> Option<ObjTask> {
    let key = key.to_owned();
    let tasks = self.tasks.lock().await;
    tasks.get(&key).cloned()
  }

  pub async fn wait_task(&self, key: &str) {
    if let Some(task) = self.get_task(key).await {
      task.wait().await;
      self.remove_task(key).await;
    }
  }
}
