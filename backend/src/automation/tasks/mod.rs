pub mod backup_data;
pub mod baseband_reboot;
pub mod device_reboot;
pub mod send_sms;

use crate::automation::traits::AutomationTaskHandler;
use std::collections::HashMap;
use std::sync::Arc;

pub struct TaskRegistry {
    handlers: HashMap<&'static str, Arc<dyn AutomationTaskHandler>>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        let mut handlers = HashMap::new();

        let h1 = Arc::new(baseband_reboot::BasebandRebootHandler) as Arc<dyn AutomationTaskHandler>;
        handlers.insert(h1.task_type(), h1);

        let h2 = Arc::new(device_reboot::DeviceRebootHandler) as Arc<dyn AutomationTaskHandler>;
        handlers.insert(h2.task_type(), h2);

        let h3 = Arc::new(send_sms::SendSmsHandler) as Arc<dyn AutomationTaskHandler>;
        handlers.insert(h3.task_type(), h3);

        let h4 = Arc::new(backup_data::BackupDataHandler) as Arc<dyn AutomationTaskHandler>;
        handlers.insert(h4.task_type(), h4);

        Self { handlers }
    }

    pub fn get(&self, task_type: &str) -> Option<Arc<dyn AutomationTaskHandler>> {
        self.handlers.get(task_type).cloned()
    }
}
